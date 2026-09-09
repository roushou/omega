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

use crate::host::StateConfig;
use omega_document::{DocumentFile, StateDocument};
use omega_host::Layout;

use crate::DaemonError;
use crate::hub::Hub;
use crate::manifest::ManifestStore;
use crate::reconcile::{
    BarProvider, ConfigProvider, EnvironmentProvider, Reconciler, ScheduleProvider, UnitProvider,
};
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

/// A handle on the converger. Cloneable, and never blocks the caller.
#[derive(Debug, Clone)]
pub struct Converger {
    queue: Arc<Queue>,
}

impl Converger {
    /// Start converging in the background.
    pub fn spawn(context: Context, shutdown: Shutdown) -> Self {
        let converger = Self {
            queue: Arc::new(Queue::default()),
        };

        let worker = Worker {
            queue: converger.queue.clone(),
            document: StateDocument::default(),
            context,
            shutdown,
        };
        tokio::spawn(worker.run());

        // Everything the daemon runs comes from the built state, so the first
        // pass reads it.
        converger.request(Work::REBUILD);
        converger
    }

    /// Ask for a pass. Returns immediately: whether one is running, pending,
    /// or neither, the request is recorded and nothing waits.
    pub fn request(&self, work: Work) {
        self.queue.push(work);
    }
}

/// Everything a pass needs to converge with.
#[derive(Debug, Clone)]
pub struct Context {
    pub layout: Layout,
    pub hub: Hub,
    pub supervisor: Supervisor,
    pub units: UnitTable,
    /// The timers the document declares. Held here rather than built per
    /// pass: a schedule that survives a convergence is one that keeps
    /// ticking through it.
    pub schedules: Schedules,
}

struct Worker {
    queue: Arc<Queue>,
    document: StateDocument,
    context: Context,
    shutdown: Shutdown,
}

impl Worker {
    async fn run(mut self) {
        loop {
            let Some(work) = self.next().await else {
                return;
            };

            if work.reload {
                match self.reload() {
                    Ok(Some(document)) => self.document = document,
                    // A fresh machine, not a broken one. The daemon keeps
                    // running with nothing to converge toward, and the build
                    // that lands triggers the pass that adopts it.
                    Ok(None) => tracing::info!("nothing built yet; run `omega build`"),
                    // A half-written or broken build must not take down a
                    // daemon that is running the last good one.
                    Err(e) => {
                        tracing::error!(error = %e, "cannot adopt the new build; keeping the running one")
                    }
                }
            }

            if let Err(e) = self.converge().await {
                tracing::warn!(error = %e, "cannot converge");
            }
        }
    }

    /// The next pass to run, or `None` when the daemon is stopping.
    async fn next(&self) -> Option<Work> {
        loop {
            if let Some(work) = self.queue.take() {
                return Some(work);
            }

            tokio::select! {
                _ = self.queue.wake.notified() => continue,
                _ = self.shutdown.wait() => return None,
            }
        }
    }

    /// Re-read everything derived from the state dir. The manifests the
    /// daemon vouches for are replaced together with the document that says
    /// what to do with them.
    fn reload(&self) -> Result<Option<StateDocument>, DaemonError> {
        let Some(config) = self.built()? else {
            return Ok(None);
        };
        let manifests = ManifestStore::load(&config, &self.context.layout)?;
        self.context.supervisor.adopt(Arc::new(manifests));

        Ok(Some(
            DocumentFile::of(&self.context.layout).read_or_default()?,
        ))
    }

    /// What the last build produced, or `None` where none has run.
    ///
    /// A state dir with no unit config is a machine `omega build` has never
    /// been run on, which is the ordinary first boot rather than a fault —
    /// reporting it as one makes every fresh install look broken. Every
    /// other way of failing to read it still is one.
    fn built(&self) -> Result<Option<StateConfig>, DaemonError> {
        match self.context.layout.file::<StateConfig>(()).read() {
            Ok(config) => Ok(Some(config)),
            Err(e) if e.is_not_found() => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    async fn converge(&self) -> Result<(), DaemonError> {
        // Nothing built is nothing to converge toward. Tearing down what is
        // running because the config went missing is the opposite of what a
        // daemon owes a desktop.
        let Some(config) = self.built()? else {
            return Ok(());
        };
        let manifests = Arc::new(ManifestStore::load(&config, &self.context.layout)?);

        Reconciler::new()
            // Config first: a unit must not be started before the settings
            // its own handshake will carry are on file.
            .with(ConfigProvider::new(
                self.context.units.clone(),
                self.context.supervisor.clone(),
            ))
            .with(UnitProvider::new(
                self.context.supervisor.clone(),
                &self.context.layout,
                config.names().cloned(),
            ))
            .with(EnvironmentProvider::new(&self.context.layout))
            // Schedules before bars, and after units: a schedule's first
            // tick is immediate, and one that invokes a unit wants the unit
            // already started.
            .with(ScheduleProvider::new(self.context.schedules.clone()))
            // Bars last: a widget instance can only be rendered by a unit
            // that is already running.
            .with(BarProvider::new(
                self.context.hub.clone(),
                self.context.units.clone(),
                manifests,
            ))
            .converge(&self.document)
            .await;

        self.context.supervisor.publish_status();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

        /// A worker over this dir, with nothing built in it yet.
        fn worker(&self) -> Worker {
            let hub = Hub::new();
            let units = UnitTable::detached(hub.clone());
            let shutdown = Shutdown::new();

            Worker {
                queue: Arc::new(Queue::default()),
                document: StateDocument::default(),
                context: Context {
                    layout: self.layout(),
                    hub: hub.clone(),
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

    #[test]
    fn a_machine_with_no_build_has_nothing_to_adopt() {
        // The ordinary first boot. `omega build` has not run, so there is no
        // unit config to read — which was reported as a failure to load one,
        // and made every fresh install log an error about the state it was
        // supposed to be in.
        let dir = TempDir::new("unbuilt");
        let worker = dir.worker();

        assert!(worker.built().unwrap().is_none());
        assert!(matches!(worker.reload(), Ok(None)));
    }

    #[tokio::test]
    async fn converging_toward_no_build_does_nothing_rather_than_failing() {
        let dir = TempDir::new("unbuilt-converge");
        let worker = dir.worker();

        assert!(worker.converge().await.is_ok());
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
        assert!(worker.built().is_err());
        assert!(worker.reload().is_err());
    }
}
