//! Spawns plugins as subprocesses, vouches for their identity, and reports
//! their health.

pub mod backoff;
pub mod log;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use tokio::process::Child;
use tokio::sync::watch;
use tokio::time::Instant;

use omega_proto::PluginName;
use omega_proto::Socket;
use omega_proto::omega::PluginStatus;

use crate::manifest::ManifestStore;
use crate::plugins::{PluginControl, PluginRegistry, SpawnToken, Transition};
use crate::process::ManagedChild;
use crate::shutdown::Shutdown;

pub use backoff::Backoff;
pub use log::PluginLog;

/// A plugin the supervisor should run.
#[derive(Debug, Clone)]
pub struct PluginSpec {
    pub name: PluginName,
    pub program: PathBuf,
    generation: Option<omega_host::Generation>,
    /// Captured output path. If absent, inherit the daemon's output streams.
    pub log: Option<PluginLog>,
}

impl PluginSpec {
    pub fn new(name: PluginName, program: impl Into<PathBuf>) -> Self {
        Self {
            name,
            program: program.into(),
            generation: None,
            log: None,
        }
    }

    /// Keep the executable generation leased throughout supervision and crash recovery.
    pub fn for_generation(name: PluginName, generation: omega_host::Generation) -> Self {
        let mut spec = Self::new(
            name.clone(),
            generation.layout().state_plugin_program(&name),
        );
        spec.generation = Some(generation);
        spec
    }

    /// Send this plugin's output to its own log.
    pub fn logged(mut self, log: PluginLog) -> Self {
        self.log = Some(log);
        self
    }
}

/// Spawns and restarts plugins, and answers the one question every session
/// starts with: which plugin is this, if any?
#[derive(Debug, Clone)]
pub struct Supervisor {
    inner: Arc<SupervisorInner>,
}

#[derive(Debug)]
struct SupervisorInner {
    socket: Socket,
    /// The daemon is stopping: every plugin goes with it.
    shutdown: Shutdown,
    /// Shared plugin registry; the supervisor keeps no separate copy.
    plugins: PluginRegistry,
    handover: Arc<tokio::sync::Mutex<()>>,
}

impl Supervisor {
    pub fn new(socket: Socket, plugins: PluginRegistry, shutdown: Shutdown) -> Self {
        plugins
            .hosts()
            .environment(socket.clone(), shutdown.clone());
        Self {
            inner: Arc::new(SupervisorInner {
                socket,
                shutdown,
                plugins,
                handover: Arc::new(tokio::sync::Mutex::new(())),
            }),
        }
    }

    /// What the supervisor currently reports about each plugin.
    pub fn statuses(&self) -> Vec<PluginStatus> {
        self.inner.plugins.statuses()
    }

    /// Whether every plugin has finished stopping.
    pub fn all_stopped(&self) -> bool {
        self.inner.plugins.all_stopped() && self.inner.plugins.hosts().all_stopped()
    }

    /// Register a plugin this supervisor did not spawn, for tests and dev. The
    /// returned token is what that process must present in `Hello`.
    pub fn register(&self, name: &PluginName) -> Result<SpawnToken, crate::plugins::TokenError> {
        self.inner.plugins.issue(name)
    }

    /// The plugin a connecting peer may claim. `None` means the peer is not a
    /// plugin — a stale token, a recycled pid, or a stranger on the socket.
    pub fn identify(&self, pid: i32, token: &str) -> Option<PluginName> {
        self.inner.plugins.identify(pid, token)
    }

    /// Replace manifests for future admissions. Existing sessions retain their grants.
    pub fn adopt(&self, manifests: Arc<ManifestStore>) {
        self.inner.plugins.adopt(&manifests);
    }

    /// Supervise this executable until the plugin or daemon stops.
    /// Build activation owns replacement; crash recovery reuses this exact path.
    pub fn spawn(&self, spec: PluginSpec) -> tokio::task::JoinHandle<()> {
        let control = PluginControl {
            stop: Shutdown::new(),
            cycle: watch::channel(0).0,
        };
        self.inner.plugins.supervise(&spec.name, control.clone());
        let task_name = format!("supervisor {}", spec.name);
        let process = PluginProcess::new(self.inner.clone(), spec, control);
        let shutdown = self.inner.shutdown.clone();
        tokio::spawn(async move { shutdown.supervise(task_name, process.run()).await })
    }

    /// Cycle a plugin's process, leaving it supervised. Returns whether there
    /// was a plugin to cycle.
    pub fn restart(&self, name: &PluginName) -> bool {
        let Some(control) = self.inner.plugins.control(name) else {
            return false;
        };
        tracing::info!(plugin = %name, "restart requested");
        control.cycle.send_modify(|count| *count += 1);
        true
    }

    /// Publish the current table, so observers see the topic even when the
    /// document leaves nothing to run.
    pub fn publish_status(&self) {
        self.inner.plugins.publish();
    }

    /// The plugins something is running: supervised by this process, or
    /// adopted by whoever is developing them.
    pub fn running(&self) -> Vec<PluginName> {
        self.inner.plugins.held()
    }

    /// Stop and await the supervised process before granting development adoption.
    /// Only one process may own a plugin identity at a time.
    pub async fn handover(&self) -> tokio::sync::OwnedMutexGuard<()> {
        self.inner.handover.clone().lock_owned().await
    }

    pub async fn adopt_plugin(
        &self,
        name: &PluginName,
    ) -> Result<SpawnToken, omega_proto::Refusal> {
        self.adopt_with(name, SpawnToken::mint).await
    }

    async fn adopt_with(
        &self,
        name: &PluginName,
        mint: impl FnOnce() -> Result<SpawnToken, crate::plugins::TokenError>,
    ) -> Result<SpawnToken, omega_proto::Refusal> {
        let _handover = self.handover().await;
        if self.inner.plugins.manifest(name).is_none() {
            return Err(omega_proto::Refusal::precondition(format!(
                "{name} is not a plugin this build contains"
            )));
        }
        use crate::refusal::RefusableResult;
        let token = mint().or_refuse()?;
        self.stop(name).await;
        if self.inner.plugins.is_supervised(name) {
            return Err(omega_proto::Refusal::precondition(format!(
                "{name} has not finished stopping"
            )));
        }
        Ok(self.inner.plugins.adopt_with_token(name, token))
    }

    /// Give an adopted plugin back, so the next convergence runs the binary the
    /// build produced.
    pub fn release_plugin(&self, name: &PluginName, token: &SpawnToken) {
        self.inner.plugins.release_adoption(name, token);
    }

    /// Stop one plugin and wait for it, leaving every other plugin alone.
    pub async fn stop(&self, name: &PluginName) {
        let Some(control) = self.inner.plugins.control(name) else {
            return;
        };
        control.stop.trigger();

        // Wait for supervision to end, allowing shutdown grace plus observation delay.
        let deadline =
            tokio::time::Instant::now() + PluginProcess::GRACE + Duration::from_millis(500);
        while tokio::time::Instant::now() < deadline {
            if !self.inner.plugins.is_supervised(name) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        tracing::warn!(plugin = %name, "plugin did not stop within its grace period");
    }
}

/// One supervised plugin: spawn, watch, restart, until the daemon stops.
struct PluginProcess {
    supervisor: Arc<SupervisorInner>,
    spec: PluginSpec,
    backoff: Backoff,
    /// This plugin alone: stopped by the reconciler, cycled by an operator.
    control: PluginControl,
    cycles: watch::Receiver<u64>,
}

impl PluginProcess {
    /// How long a plugin gets to exit on its own before it is killed.
    pub(crate) const GRACE: Duration = Duration::from_secs(5);

    fn new(supervisor: Arc<SupervisorInner>, spec: PluginSpec, control: PluginControl) -> Self {
        let cycles = control.cycle.subscribe();
        Self {
            supervisor,
            spec,
            backoff: Backoff::new(),
            control,
            cycles,
        }
    }

    /// Whether plugin-specific or daemon-wide shutdown was requested.
    fn stopping(&self) -> bool {
        self.control.stop.is_triggered() || self.supervisor.shutdown.is_triggered()
    }

    /// Resolves when either reason to stop arrives.
    async fn stopped(&self) {
        Self::either(&self.control.stop, &self.supervisor.shutdown).await
    }

    async fn either(stop: &Shutdown, shutdown: &Shutdown) {
        tokio::select! {
            _ = stop.wait() => {}
            _ = shutdown.wait() => {}
        }
    }

    async fn run(mut self) {
        while !self.stopping() {
            tracing::info!(plugin = %self.spec.name, program = %self.spec.program.display(), "spawning plugin");
            // The token is issued before the spawn: a plugin that connects the
            // instant it starts must already be identifiable.
            let started = Instant::now();
            let child = self
                .supervisor
                .plugins
                .issue(&self.spec.name)
                .map_err(std::io::Error::other)
                .and_then(|token| {
                    self.report(Transition::Spawned);
                    self.start(&token)
                });

            match child {
                Ok(mut child) => {
                    if let Some(pid) = child.id() {
                        self.supervisor.plugins.bind(&self.spec.name, pid as i32);
                    }

                    self.watch(&mut child).await;

                    // A plugin that stayed up is not in a crash loop, whatever
                    // it did an hour ago.
                    if started.elapsed() >= Backoff::HEALTHY {
                        self.backoff.reset();
                    }
                }
                Err(e) => {
                    // Include the binary path in spawn errors reported by status.
                    tracing::error!(
                        plugin = %self.spec.name,
                        program = %self.spec.program.display(),
                        error = %e,
                        "failed to spawn plugin"
                    );
                    self.report(Transition::Unspawnable(format!(
                        "cannot run {}: {e}",
                        self.spec.program.display()
                    )));
                }
            }

            // The process is gone; its token dies with it, so a recycled pid
            // cannot inherit this plugin's grants.
            self.supervisor.plugins.revoke(&self.spec.name);

            if self.stopping() {
                break;
            }

            let delay = self.backoff.delay();
            tracing::debug!(
                plugin = %self.spec.name,
                ?delay,
                attempt = self.backoff.attempts(),
                "restarting after backoff"
            );

            tokio::select! {
                _ = tokio::time::sleep(delay) => {}
                _ = self.stopped() => break,
            }
        }
    }

    /// Tell the table what happened. Nothing else records it.
    fn report(&self, transition: Transition) {
        self.supervisor
            .plugins
            .transition(&self.spec.name, transition);
    }

    fn start(&self, token: &SpawnToken) -> std::io::Result<Child> {
        ManagedChild::spawn(
            &self.spec.program,
            &self.supervisor.socket,
            token,
            self.spec.generation.as_ref(),
            self.spec.log.as_ref(),
        )
    }

    /// Watch one child until it exits or receives a stop or restart request.
    async fn watch(&mut self, child: &mut Child) {
        let stop = self.control.stop.clone();
        let shutdown = self.supervisor.shutdown.clone();

        tokio::select! {
            _ = Self::either(&stop, &shutdown) => {
                self.terminate(child).await;
            }
            // Requested restarts bypass crash backoff.
            _ = self.cycles.changed() => {
                tracing::info!(plugin = %self.spec.name, "cycling on request");
                self.terminate(child).await;
                self.backoff.reset();
            }
            status = child.wait() => {
                self.exited(status);
            }
        }
    }

    fn exited(&self, status: std::io::Result<std::process::ExitStatus>) {
        match status {
            Ok(status) => {
                tracing::warn!(plugin = %self.spec.name, %status, "plugin exited; restarting");
                self.report(Transition::Exited {
                    code: status.code().unwrap_or(-1),
                    detail: status.to_string(),
                });
            }
            Err(e) => {
                tracing::error!(
                    plugin = %self.spec.name,
                    program = %self.spec.program.display(),
                    error = %e,
                    "wait failed"
                );
                self.report(Transition::Unspawnable(format!(
                    "cannot wait on {}: {e}",
                    self.spec.program.display()
                )));
            }
        }
    }

    /// Send SIGTERM, wait for the grace period, then kill if needed.
    async fn terminate(&self, child: &mut Child) {
        if let Err(error) = ManagedChild::stop(child).await {
            tracing::error!(plugin = %self.spec.name, %error, "could not reap plugin process");
            self.supervisor.shutdown.trigger();
        }
    }
}

impl Drop for PluginProcess {
    fn drop(&mut self) {
        self.supervisor.plugins.revoke(&self.spec.name);
        self.report(Transition::Stopped);
        self.supervisor.plugins.release(&self.spec.name);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn failed_adoption_identity_preserves_the_supervised_session() {
        let name: PluginName = "test".parse().unwrap();
        let plugins = PluginRegistry::detached(crate::hub::Hub::new());
        plugins.adopt(&ManifestStore::from_manifests([
            omega_proto::Manifest::new(&name, "1"),
        ]));
        let token = plugins.issue(&name).unwrap();
        plugins.bind(&name, 123);
        let stop = Shutdown::new();
        plugins.supervise(
            &name,
            PluginControl {
                stop: stop.clone(),
                cycle: watch::channel(0).0,
            },
        );
        let (requests, _receiver) = tokio::sync::mpsc::channel(1);
        let _session = plugins.connected(&name, requests);
        let supervisor =
            Supervisor::new(Socket::at("/unused.sock"), plugins.clone(), Shutdown::new());
        let before = plugins.statuses();
        let error = supervisor
            .adopt_with(&name, || Err(getrandom::Error::UNSUPPORTED.into()))
            .await
            .unwrap_err();
        assert_eq!(error.code, omega_proto::omega::ErrorCode::Unavailable);
        assert!(!stop.is_triggered());
        assert!(plugins.is_supervised(&name));
        assert!(plugins.is_connected(&name));
        assert!(!plugins.is_adopted(&name));
        assert_eq!(plugins.identify(123, token.as_str()), Some(name));
        assert_eq!(plugins.statuses(), before);
    }
}
