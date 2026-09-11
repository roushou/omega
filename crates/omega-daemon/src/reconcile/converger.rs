//! The one thing that converges.
//!
//! Convergence is triggered from several places — the daemon starting, a
//! rebuild landing, a unit connecting — and it can take a while: rendering a
//! bar instance waits on a unit, which may be slow or wedged. Two things
//! follow from that, and both are structural rather than careful:
//!
//! - it runs in its own task, so a slow unit cannot stop the daemon from
//!   accepting connections or answering a signal;
//! - it runs *one at a time*, and requests that arrive while it is working
//!   are merged into the next pass rather than starting a second one.
//!
//! So a trigger is a request, never a call.

use std::sync::{Arc, Mutex};

use tokio::sync::Notify;

use omega_host::Layout;

use super::build::ValidatedBuild;
use crate::DaemonError;
use crate::reconcile::{BarProvider, EnvironmentProvider, ScheduleProvider, UnitProvider};
use crate::schedule::Schedules;
use crate::shutdown::Shutdown;
use crate::supervisor::Supervisor;
use crate::units::UnitTable;

/// What a pass has to do. Merging is a union: a reload asked for while a
/// plain convergence is pending upgrades the pending one rather than queuing
/// behind it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Work {
    /// Re-read the built state: what was built, what it is for, and the
    /// manifests the daemon vouches for.
    pub reload: bool,
}

impl Work {
    /// Plan against what is already loaded.
    pub const CONVERGE: Self = Self { reload: false };
    /// A build landed; everything derived from the state dir is stale.
    pub const REBUILD: Self = Self { reload: true };

    pub fn merge(self, other: Self) -> Self {
        Self {
            reload: self.reload || other.reload,
        }
    }
}

/// The queue of pending work: at most one pass, however many triggers.
#[derive(Debug, Default)]
struct Queue {
    pending: Mutex<Option<Work>>,
    wake: Notify,
}

impl Queue {
    fn push(&self, work: Work) {
        let mut pending = self.lock();
        *pending = Some(pending.map_or(work, |queued| queued.merge(work)));
        drop(pending);

        // `Notify` holds a permit, so a request that arrives before the
        // worker waits is not lost.
        self.wake.notify_one();
    }

    fn take(&self) -> Option<Work> {
        self.lock().take()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Option<Work>> {
        self.pending.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// Owns the convergence task; requesting work never blocks the caller.
#[derive(Debug)]
pub struct Converger {
    queue: Arc<Queue>,
    task: tokio::task::JoinHandle<()>,
}

impl Converger {
    /// Start converging in the background.
    pub fn spawn(context: Context, shutdown: Shutdown) -> Self {
        let queue = Arc::new(Queue::default());
        let worker = Worker {
            queue: queue.clone(),
            build: None,
            context,
            shutdown: shutdown.clone(),
        };
        let task = tokio::spawn(async move {
            shutdown
                .supervise("convergence".into(), async {
                    tokio::select! {
                        biased;
                        _ = shutdown.wait() => {}
                        _ = worker.run() => {}
                    }
                })
                .await;
        });
        let converger = Self { queue, task };

        // Everything the daemon runs comes from the built state, so the first
        // pass reads it.
        converger.request(Work::REBUILD);
        converger
    }

    /// Cancel any in-progress pass and wait for its resources to be released.
    pub async fn stop(mut self) {
        self.task.abort();
        if let Err(error) = (&mut self.task).await
            && !error.is_cancelled()
        {
            tracing::error!(%error, "convergence task failed");
        }
    }

    /// Ask for a pass. Returns immediately: whether one is running, pending,
    /// or neither, the request is recorded and nothing waits.
    pub fn request(&self, work: Work) {
        self.queue.push(work);
    }
}

impl Drop for Converger {
    fn drop(&mut self) {
        self.task.abort();
    }
}

/// Everything a pass needs to converge with.
#[derive(Debug, Clone)]
pub struct Context {
    pub layout: Layout,
    pub supervisor: Supervisor,
    pub units: UnitTable,
    /// The timers the document declares. Held here rather than built per
    /// pass: a schedule that survives a convergence is one that keeps
    /// ticking through it.
    pub schedules: Schedules,
}

struct Worker {
    queue: Arc<Queue>,
    build: Option<ValidatedBuild>,
    context: Context,
    shutdown: Shutdown,
}

impl Worker {
    async fn run(mut self) {
        let mut retry = false;
        let mut reload_pending = false;
        loop {
            let Some(work) = self.next(retry).await else {
                return;
            };

            if work.reload || reload_pending {
                match self.reload() {
                    Ok(Some(build)) => {
                        reload_pending = match self.activate(build).await {
                            Ok(()) => false,
                            Err(error) => {
                                tracing::warn!(%error, "build activation remains pending");
                                true
                            }
                        };
                    }
                    // A fresh machine, not a broken one. The daemon keeps
                    // running with nothing to converge toward, and the build
                    // that lands triggers the pass that adopts it.
                    Ok(None) => {
                        reload_pending = false;
                        tracing::info!("nothing built yet; run `omega build`");
                    }
                    // A half-written or broken build must not take down a
                    // daemon that is running the last good one.
                    Err(e) => {
                        reload_pending = false;
                        tracing::error!(error = %e, "cannot adopt the new build; keeping the running one")
                    }
                }
            }

            retry = reload_pending;
            if let Err(e) = self.converge().await {
                tracing::warn!(error = %e, "convergence remains pending");
                retry = true;
            }
        }
    }

    /// The next pass to run, or `None` when the daemon is stopping.
    async fn next(&self, retry: bool) -> Option<Work> {
        loop {
            if let Some(work) = self.queue.take() {
                return Some(work);
            }

            tokio::select! {
                _ = self.queue.wake.notified() => continue,
                _ = tokio::time::sleep(std::time::Duration::from_secs(2)), if retry => return Some(Work::CONVERGE),
                _ = self.shutdown.wait() => return None,
            }
        }
    }

    /// Re-read everything derived from the state dir. The manifests the
    /// daemon vouches for are replaced together with the document that says
    /// what to do with them.
    fn reload(&self) -> Result<Option<ValidatedBuild>, DaemonError> {
        let candidate = ValidatedBuild::load(&self.context.layout);
        if self.build.is_some() || matches!(candidate, Ok(Some(_))) {
            return candidate;
        }
        let store = omega_host::Generations::new(&self.context.layout);
        for id in store.recovery_ids()? {
            match store
                .pin(&id)
                .map_err(DaemonError::from)
                .and_then(ValidatedBuild::read)
            {
                Ok(build) => {
                    if let Err(error) = &candidate {
                        tracing::warn!(%error, "published generation rejected at startup");
                    }
                    tracing::warn!(generation = %id, "recovering accepted generation");
                    return Ok(Some(build));
                }
                Err(error) => {
                    tracing::error!(generation = %id, %error, "recovery generation rejected")
                }
            }
        }
        candidate
    }

    async fn activate(&mut self, build: ValidatedBuild) -> Result<(), DaemonError> {
        let _handover = self.context.supervisor.handover().await;
        let changed = match &self.build {
            Some(previous) => previous.changed_units(&build)?,
            None => Vec::new(),
        };
        for name in &changed {
            if self.context.units.is_adopted(name) {
                return Err(std::io::Error::other(format!(
                    "{name} is held by omega dev; disconnect it before activating this build"
                ))
                .into());
            }
        }
        for name in &changed {
            self.context.supervisor.stop(name).await;
            if self.context.units.is_supervised(name) {
                return Err(
                    std::io::Error::other(format!("{name} has not finished stopping")).into(),
                );
            }
            self.context.units.revoke(name);
        }
        build.generation.accept()?;
        self.context.units.activate(
            &build.manifests,
            build
                .config
                .names()
                .map(|name| (name.clone(), build.settings(name)))
                .collect(),
        );
        // Shell conflicts must not stop otherwise valid plugins. Explicit apply reports
        // failures to the operator; routine convergence never rewrites external edits.
        if let Err(error) = build.apply_shell(&self.context.layout, false) {
            tracing::error!(%error, "shell configuration was not applied; use omega shell diff");
        }
        self.build = Some(build);
        Ok(())
    }

    async fn converge(&self) -> Result<(), DaemonError> {
        let Some(build) = &self.build else {
            return Ok(());
        };

        let units = UnitProvider::new(
            self.context.supervisor.clone(),
            build.generation.clone(),
            build.config.names().cloned(),
        );
        let environment = EnvironmentProvider::new(&self.context.layout);
        let schedules = ScheduleProvider::new(self.context.schedules.clone());
        let bars = BarProvider::new(self.context.units.clone(), build.manifests.clone());

        // Validate every plan before the first effect. A failed application
        // stops the pass; the retry plans again against actual ownership.
        let unit_changes = units.plan(&build.document)?;
        let environment_change = environment.plan(&build.document)?;
        let schedule_changes = schedules.plan(&build.document)?;
        let instance_changes = bars.plan(&build.document)?;

        units.apply(&unit_changes).await?;
        if let Some(change) = environment_change {
            environment.apply(&change)?;
        }
        // The first tick is immediate, so units must be supervised first.
        schedules.apply(&schedule_changes).await?;
        bars.apply(&instance_changes).await?;

        self.context.supervisor.publish_status();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hub::Hub;
    use omega_document::DocumentFile;
    use omega_host::{Generations, StateConfig};

    use std::path::PathBuf;

    use omega_proto::Socket;

    use crate::broker::Brokerage;

    #[test]
    fn triggers_that_arrive_together_are_one_pass() {
        let queue = Queue::default();

        queue.push(Work::CONVERGE);
        queue.push(Work::CONVERGE);
        queue.push(Work::CONVERGE);

        assert_eq!(queue.take(), Some(Work::CONVERGE));
        assert_eq!(queue.take(), None, "one pass answers them all");
    }

    #[test]
    fn a_rebuild_upgrades_a_pending_pass() {
        let queue = Queue::default();

        // A unit connected, and then a build landed before the pass ran: the
        // pass has to re-read the build, not plan against the old one.
        queue.push(Work::CONVERGE);
        queue.push(Work::REBUILD);
        assert_eq!(queue.take(), Some(Work::REBUILD));

        // ...and the other way round: a plain convergence does not downgrade
        // a pending rebuild.
        queue.push(Work::REBUILD);
        queue.push(Work::CONVERGE);
        assert_eq!(queue.take(), Some(Work::REBUILD));
    }

    /// A state dir that cleans up after itself.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(tag: &str) -> Self {
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let dir =
                std::env::temp_dir().join(format!("omega-{tag}-{}-{nanos}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }

        fn layout(&self) -> Layout {
            Layout::at(
                self.0.join("config"),
                self.0.join("state"),
                self.0.join("cache"),
            )
        }

        fn publish_unit(&self, contents: &[u8]) -> Layout {
            use std::os::unix::fs::PermissionsExt;
            let root = self.layout();
            let generation = Generations::new(&root).stage().unwrap();
            let layout = Layout::at(&root.config, generation.files().path(), &root.cache);
            let name = omega_proto::UnitName::parse("example").unwrap();
            let manifest =
                omega_proto::Manifest::new(&name, "1").exposing([omega_proto::Surface::new(
                    &omega_proto::SurfaceId::parse("view").unwrap(),
                    omega_proto::omega::SurfaceKind::Widget,
                )]);
            generation
                .files()
                .write(layout.unit_program_rel(&name), contents)
                .unwrap();
            std::fs::set_permissions(
                layout.state_unit_program(&name),
                std::fs::Permissions::from_mode(0o755),
            )
            .unwrap();
            generation
                .files()
                .write(layout.unit_manifest_rel(&name), &manifest.canonical())
                .unwrap();
            generation
                .files()
                .file::<StateConfig>(StateConfig::FILE_NAME)
                .write(&StateConfig::new(&layout, [name]))
                .unwrap();
            generation
                .files()
                .write(DocumentFile::FILE_NAME, b"{}")
                .unwrap();
            generation.commit().unwrap()
        }

        /// A worker over this dir, with nothing built in it yet.
        fn worker(&self) -> Worker {
            let hub = Hub::new();
            let units = UnitTable::detached(hub.clone());
            let shutdown = Shutdown::new();

            Worker {
                queue: Arc::new(Queue::default()),
                build: None,
                context: Context {
                    layout: self.layout(),
                    // Never bound: nothing here spawns a unit.
                    supervisor: Supervisor::new(
                        Socket::at(self.0.join("omega.sock")),
                        units.clone(),
                        shutdown.clone(),
                    ),
                    // Never fired: nothing here converges a document.
                    schedules: Schedules::new(
                        hub.clone(),
                        units.clone(),
                        Brokerage::new(hub, shutdown.clone()),
                        shutdown.clone(),
                    ),
                    units,
                },
                shutdown,
            }
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[tokio::test(start_paused = true)]
    async fn shutdown_cancels_activation_waiting_for_handover() {
        let dir = TempDir::new("converger-shutdown");
        dir.publish_unit(b"unused");
        let worker = dir.worker();
        let supervisor = worker.context.supervisor.clone();
        let _handover = supervisor.handover().await;
        let shutdown = worker.shutdown.clone();
        let mut converger = Converger::spawn(worker.context, shutdown.clone());
        tokio::task::yield_now().await;
        assert!(!converger.task.is_finished());
        shutdown.trigger();
        tokio::time::timeout(std::time::Duration::from_secs(1), &mut converger.task)
            .await
            .unwrap()
            .unwrap();
        assert!(converger.task.is_finished());
    }

    #[tokio::test]
    async fn dropping_the_owner_cancels_convergence() {
        let dir = TempDir::new("converger-drop");
        let worker = dir.worker();
        let converger = Converger::spawn(worker.context, worker.shutdown);
        let task = converger.task.abort_handle();
        drop(converger);
        tokio::task::yield_now().await;
        assert!(task.is_finished());
    }

    #[test]
    fn a_machine_with_no_build_has_nothing_to_adopt() {
        // The ordinary first boot. `omega build` has not run, so there is no
        // unit config to read — which was reported as a failure to load one,
        // and made every fresh install log an error about the state it was
        // supposed to be in.
        let dir = TempDir::new("unbuilt");
        let worker = dir.worker();

        assert!(matches!(worker.reload(), Ok(None)));
    }

    #[tokio::test]
    async fn converging_toward_no_build_does_nothing_rather_than_failing() {
        let dir = TempDir::new("unbuilt-converge");
        let worker = dir.worker();

        assert!(worker.converge().await.is_ok());
    }

    #[tokio::test]
    async fn accepted_inputs_survive_a_broken_reload() {
        let dir = TempDir::new("accepted-build");
        let layout = dir.layout();
        let generation = Generations::new(&layout).stage().unwrap();
        generation
            .files()
            .file::<StateConfig>(StateConfig::FILE_NAME)
            .write(&StateConfig::default())
            .unwrap();
        generation
            .files()
            .write(DocumentFile::FILE_NAME, b"{}")
            .unwrap();
        let accepted = generation.commit().unwrap();
        let mut worker = dir.worker();
        worker.build = worker.reload().unwrap();
        let generation = Generations::new(&layout).stage().unwrap();
        generation
            .files()
            .file::<StateConfig>(StateConfig::FILE_NAME)
            .write(&StateConfig::default())
            .unwrap();
        generation
            .files()
            .write(DocumentFile::FILE_NAME, b"broken JSON")
            .unwrap();
        generation.commit().unwrap();
        assert!(worker.reload().is_err());
        assert_eq!(
            worker.build.as_ref().unwrap().generation.layout().state,
            accepted.state
        );
        assert!(worker.converge().await.is_ok());
        assert!(worker.build.as_ref().unwrap().manifests.is_empty());
    }

    #[tokio::test]
    async fn a_development_lease_blocks_changed_builds_until_released() {
        let dir = TempDir::new("generation-dev-hold");
        let first = dir.publish_unit(b"first");
        let mut worker = dir.worker();
        worker
            .activate(worker.reload().unwrap().unwrap())
            .await
            .unwrap();
        let name = omega_proto::UnitName::parse("example").unwrap();
        let token = worker.context.units.adopt_unit(&name);
        let second = dir.publish_unit(b"second");
        assert!(
            worker
                .activate(worker.reload().unwrap().unwrap())
                .await
                .is_err()
        );
        assert_eq!(
            worker.build.as_ref().unwrap().generation.layout().state,
            first.state
        );
        worker.context.units.release_adoption(&name, &token);
        worker
            .activate(worker.reload().unwrap().unwrap())
            .await
            .unwrap();
        assert_eq!(
            worker.build.as_ref().unwrap().generation.layout().state,
            second.state
        );
    }

    #[tokio::test]
    async fn failed_acceptance_keeps_live_inputs_and_retries_after_repair() {
        let dir = TempDir::new("acceptance-failure");
        let first = dir.publish_unit(b"first");
        let mut worker = dir.worker();
        worker
            .activate(worker.reload().unwrap().unwrap())
            .await
            .unwrap();
        let history = dir.layout().generation_history();
        let saved = std::fs::read(&history).unwrap();
        let second = dir.publish_unit(b"second");
        omega_host::AtomicFile::at(&history)
            .write(b"invalid history {")
            .unwrap();
        assert!(
            worker
                .activate(worker.reload().unwrap().unwrap())
                .await
                .is_err()
        );
        assert_eq!(
            worker.build.as_ref().unwrap().generation.layout().state,
            first.state
        );
        omega_host::AtomicFile::at(&history).write(&saved).unwrap();
        worker
            .activate(worker.reload().unwrap().unwrap())
            .await
            .unwrap();
        assert_eq!(
            worker.build.as_ref().unwrap().generation.layout().state,
            second.state
        );
        assert_eq!(
            Generations::new(&dir.layout())
                .recovery_ids()
                .unwrap()
                .len(),
            2
        );
    }

    #[tokio::test(start_paused = true)]
    async fn a_held_disabled_unit_blocks_dependents_until_release_and_retry() {
        let dir = TempDir::new("convergence-retry");
        dir.publish_unit(b"unused");
        let mut worker = dir.worker();
        worker
            .activate(worker.reload().unwrap().unwrap())
            .await
            .unwrap();
        let name = omega_proto::UnitName::parse("example").unwrap();
        let token = worker.context.units.adopt_unit(&name);
        let document = &mut worker.build.as_mut().unwrap().document;
        *document = omega_document::Document::new()
            .unit(omega_document::Units::disabled("example"))
            .env("EDITOR", "hx")
            .into_inner();
        document
            .schedules
            .push(omega_proto::omega::Schedule::announcing(
                "tick",
                omega_proto::Cadence::seconds(10),
            ));
        let environment = dir.layout().environment();
        assert!(worker.converge().await.is_err());
        assert!(!environment.exists());
        assert!(worker.context.schedules.declared().is_empty());

        worker.context.units.release_adoption(&name, &token);
        let hub = Hub::new();
        worker.context.schedules = Schedules::new(
            hub.clone(),
            worker.context.units.clone(),
            Brokerage::new(hub.clone(), worker.shutdown.clone()),
            worker.shutdown.clone(),
        );
        let mut events = hub.subscribe_events();
        worker.converge().await.unwrap();
        assert_eq!(
            std::fs::read_to_string(&environment).unwrap(),
            "EDITOR=hx\n"
        );
        events.recv().await.unwrap();
        tokio::time::advance(std::time::Duration::from_secs(5)).await;
        worker.converge().await.unwrap();
        tokio::task::yield_now().await;
        assert!(matches!(
            events.try_recv(),
            Err(tokio::sync::broadcast::error::TryRecvError::Empty)
        ));
        tokio::time::advance(std::time::Duration::from_secs(5)).await;
        tokio::task::yield_now().await;
        assert_eq!(events.try_recv().unwrap().schedule(), Some("tick"));
        worker.context.schedules.shutdown().await;
    }

    #[tokio::test]
    async fn a_plan_failure_starts_no_units_and_can_be_retried_after_repair() {
        let dir = TempDir::new("convergence-plan-failure");
        dir.publish_unit(b"unused");
        let mut worker = dir.worker();
        worker
            .activate(worker.reload().unwrap().unwrap())
            .await
            .unwrap();
        let path = dir.layout().environment();
        std::fs::create_dir(&path).unwrap();
        assert!(worker.converge().await.is_err());
        assert!(worker.context.supervisor.running().is_empty());
        std::fs::remove_dir(&path).unwrap();
        worker.build.as_mut().unwrap().document = omega_document::Document::new()
            .unit(omega_document::Units::disabled("example"))
            .into_inner();
        worker.converge().await.unwrap();
    }

    #[test]
    fn a_unit_config_that_cannot_be_parsed_is_still_a_failure() {
        // The half that keeps the two above honest: absent and unreadable are
        // different, and only the first one is ordinary.
        let dir = TempDir::new("unbuilt-broken");
        let path = dir.layout().file::<StateConfig>(()).into_path();
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "this is not toml {").unwrap();

        let worker = dir.worker();
        assert!(worker.reload().is_err());
    }
}
