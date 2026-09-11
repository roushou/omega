//! Spawns units as subprocesses, vouches for their identity, and reports
//! their health.

pub mod backoff;
pub mod log;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use tokio::process::{Child, Command};
use tokio::sync::watch;
use tokio::time::Instant;

use omega_proto::UnitName;
use omega_proto::omega::UnitStatus;
use omega_proto::{Handshake, Socket};

use crate::manifest::ManifestStore;
use crate::process::Signal;
use crate::shutdown::Shutdown;
use crate::units::{Transition, UnitControl, UnitTable, UnitToken};

pub use backoff::Backoff;
pub use log::UnitLog;

/// A unit the supervisor should run.
#[derive(Debug, Clone)]
pub struct UnitSpec {
    pub name: UnitName,
    pub program: PathBuf,
    generation: Option<omega_host::Generation>,
    /// Where the unit's own output goes. Absent means it inherits the
    /// daemon's, which is what a test wants and a desktop does not.
    pub log: Option<UnitLog>,
}

impl UnitSpec {
    pub fn new(name: UnitName, program: impl Into<PathBuf>) -> Self {
        Self {
            name,
            program: program.into(),
            generation: None,
            log: None,
        }
    }

    /// Keep the executable generation leased throughout supervision and crash recovery.
    pub fn for_generation(name: UnitName, generation: omega_host::Generation) -> Self {
        let mut spec = Self::new(name.clone(), generation.layout().state_unit_program(&name));
        spec.generation = Some(generation);
        spec
    }

    /// Send this unit's output to its own log.
    pub fn logged(mut self, log: UnitLog) -> Self {
        self.log = Some(log);
        self
    }
}

/// Spawns and restarts units, and answers the one question every session
/// starts with: which unit is this, if any?
#[derive(Debug, Clone)]
pub struct Supervisor {
    inner: Arc<SupervisorInner>,
}

#[derive(Debug)]
struct SupervisorInner {
    socket: Socket,
    /// The daemon is stopping: every unit goes with it.
    shutdown: Shutdown,
    /// Everything known about the units — manifests, tokens, lifecycles, and
    /// the handles that stop and cycle them. The supervisor keeps no private
    /// copy of any of it.
    units: UnitTable,
    handover: Arc<tokio::sync::Mutex<()>>,
}

impl Supervisor {
    pub fn new(socket: Socket, units: UnitTable, shutdown: Shutdown) -> Self {
        Self {
            inner: Arc::new(SupervisorInner {
                socket,
                shutdown,
                units,
                handover: Arc::new(tokio::sync::Mutex::new(())),
            }),
        }
    }

    /// What the supervisor currently reports about each unit.
    pub fn statuses(&self) -> Vec<UnitStatus> {
        self.inner.units.statuses()
    }

    /// Whether every unit has finished stopping.
    pub fn all_stopped(&self) -> bool {
        self.inner.units.all_stopped()
    }

    /// Register a unit this supervisor did not spawn, for tests and dev. The
    /// returned token is what that process must present in `Hello`.
    pub fn register(&self, name: &UnitName) -> UnitToken {
        self.inner.units.issue(name)
    }

    /// The unit a connecting peer may claim. `None` means the peer is not a
    /// unit — a stale token, a recycled pid, or a stranger on the socket.
    pub fn identify(&self, pid: i32, token: &str) -> Option<UnitName> {
        self.inner.units.identify(pid, token)
    }

    /// Adopt the manifests of a fresh build. Sessions already open keep the
    /// grants they were admitted with; the next handshake is checked against
    /// what is on disk now.
    pub fn adopt(&self, manifests: Arc<ManifestStore>) {
        self.inner.units.adopt(&manifests);
    }

    /// Supervise this executable until the unit or daemon stops.
    /// Build activation owns replacement; crash recovery reuses this exact path.
    pub fn spawn(&self, spec: UnitSpec) -> tokio::task::JoinHandle<()> {
        let control = UnitControl {
            stop: Shutdown::new(),
            cycle: watch::channel(0).0,
        };
        self.inner.units.supervise(&spec.name, control.clone());
        let task_name = format!("supervisor {}", spec.name);
        let process = UnitProcess::new(self.inner.clone(), spec, control);
        let shutdown = self.inner.shutdown.clone();
        tokio::spawn(async move { shutdown.supervise(task_name, process.run()).await })
    }

    /// Cycle a unit's process, leaving it supervised. Returns whether there
    /// was a unit to cycle.
    pub fn restart(&self, name: &UnitName) -> bool {
        let Some(control) = self.inner.units.control(name) else {
            return false;
        };
        tracing::info!(unit = %name, "restart requested");
        control.cycle.send_modify(|count| *count += 1);
        true
    }

    /// Publish the current table, so observers see the topic even when the
    /// document leaves nothing to run.
    pub fn publish_status(&self) {
        self.inner.units.publish();
    }

    /// The units something is running: supervised by this process, or
    /// adopted by whoever is developing them.
    pub fn running(&self) -> Vec<UnitName> {
        self.inner.units.held()
    }

    /// Hand a unit over to a process this daemon will not spawn.
    ///
    /// The supervised instance is stopped first and waited for: the point of
    /// adopting a unit is to *be* it, and two processes answering to one name
    /// would race for its session and its surfaces.
    pub async fn handover(&self) -> tokio::sync::OwnedMutexGuard<()> {
        self.inner.handover.clone().lock_owned().await
    }

    pub async fn adopt_unit(&self, name: &UnitName) -> Result<UnitToken, omega_proto::Refusal> {
        let _handover = self.handover().await;
        if self.inner.units.manifest(name).is_none() {
            return Err(omega_proto::Refusal::precondition(format!(
                "{name} is not a unit this build contains"
            )));
        }
        self.stop(name).await;
        if self.inner.units.is_supervised(name) {
            return Err(omega_proto::Refusal::precondition(format!(
                "{name} has not finished stopping"
            )));
        }
        Ok(self.inner.units.adopt_unit(name))
    }

    /// Give an adopted unit back, so the next convergence runs the binary the
    /// build produced.
    pub fn release_unit(&self, name: &UnitName, token: &UnitToken) {
        self.inner.units.release_adoption(name, token);
    }

    /// Stop one unit and wait for it, leaving every other unit alone.
    pub async fn stop(&self, name: &UnitName) {
        let Some(control) = self.inner.units.control(name) else {
            return;
        };
        control.stop.trigger();

        // The unit gets the same grace as it would at shutdown, plus the
        // time the supervisor takes to notice. Supervision ending is the
        // signal, not a phase reported after the fact.
        let deadline =
            tokio::time::Instant::now() + UnitProcess::GRACE + Duration::from_millis(500);
        while tokio::time::Instant::now() < deadline {
            if !self.inner.units.is_supervised(name) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        tracing::warn!(unit = %name, "unit did not stop within its grace period");
    }
}

/// One supervised unit: spawn, watch, restart, until the daemon stops.
struct UnitProcess {
    supervisor: Arc<SupervisorInner>,
    spec: UnitSpec,
    backoff: Backoff,
    /// This unit alone: stopped by the reconciler, cycled by an operator.
    control: UnitControl,
    cycles: watch::Receiver<u64>,
}

impl UnitProcess {
    /// How long a unit gets to exit on its own before it is killed.
    pub(crate) const GRACE: Duration = Duration::from_secs(5);

    fn new(supervisor: Arc<SupervisorInner>, spec: UnitSpec, control: UnitControl) -> Self {
        let cycles = control.cycle.subscribe();
        Self {
            supervisor,
            spec,
            backoff: Backoff::new(),
            control,
            cycles,
        }
    }

    /// Whether this unit should stop — because it was stopped, or because
    /// the whole daemon is.
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
            tracing::info!(unit = %self.spec.name, program = %self.spec.program.display(), "spawning unit");
            self.report(Transition::Spawned);

            // The token is issued before the spawn: a unit that connects the
            // instant it starts must already be identifiable.
            let token = self.supervisor.units.issue(&self.spec.name);
            let started = Instant::now();

            match self.start(&token) {
                Ok(mut child) => {
                    if let Some(pid) = child.id() {
                        self.supervisor.units.bind(&self.spec.name, pid as i32);
                    }

                    self.watch(&mut child).await;

                    // A unit that stayed up is not in a crash loop, whatever
                    // it did an hour ago.
                    if started.elapsed() >= Backoff::HEALTHY {
                        self.backoff.reset();
                    }
                }
                Err(e) => {
                    // Named, both times. "No such file or directory" reaches
                    // `omega status` as a phase with no subject, and which
                    // path was missing is the whole diagnosis: a build that
                    // never staged the binary looks exactly like this.
                    tracing::error!(
                        unit = %self.spec.name,
                        program = %self.spec.program.display(),
                        error = %e,
                        "failed to spawn unit"
                    );
                    self.report(Transition::Unspawnable(format!(
                        "cannot run {}: {e}",
                        self.spec.program.display()
                    )));
                }
            }

            // The process is gone; its token dies with it, so a recycled pid
            // cannot inherit this unit's grants.
            self.supervisor.units.revoke(&self.spec.name);

            if self.stopping() {
                break;
            }

            let delay = self.backoff.delay();
            tracing::debug!(
                unit = %self.spec.name,
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
            .units
            .transition(&self.spec.name, transition);
    }

    fn start(&self, token: &UnitToken) -> std::io::Result<Child> {
        let mut command = Command::new(&self.spec.program);
        command
            .env("OMEGA_SOCKET", self.supervisor.socket.path())
            .env(Handshake::TOKEN_ENV, token.as_str())
            .kill_on_drop(true);

        if let Some(log) = &self.spec.log {
            let (out, err) = log.streams()?;
            command.stdout(out).stderr(err);
        }

        if let Some(generation) = &self.spec.generation {
            generation.protect_child(command.as_std_mut());
        }
        command.spawn()
    }

    /// Watch one child until it exits or receives a stop or restart request.
    async fn watch(&mut self, child: &mut Child) {
        let stop = self.control.stop.clone();
        let shutdown = self.supervisor.shutdown.clone();

        tokio::select! {
            _ = Self::either(&stop, &shutdown) => {
                self.terminate(child).await;
            }
            // An operator asked for this instance to go. The unit is
            // still wanted, so the loop respawns it — and an asked-for
            // restart is not a crash, so it does not count against the
            // backoff.
            _ = self.cycles.changed() => {
                tracing::info!(unit = %self.spec.name, "cycling on request");
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
                tracing::warn!(unit = %self.spec.name, %status, "unit exited; restarting");
                self.report(Transition::Exited {
                    code: status.code().unwrap_or(-1),
                    detail: status.to_string(),
                });
            }
            Err(e) => {
                tracing::error!(
                    unit = %self.spec.name,
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

    /// Ask the unit to exit, then insist. A unit holding a socket deserves
    /// the chance to close it; one that ignores the request does not hold up
    /// the shutdown.
    async fn terminate(&self, child: &mut Child) {
        let Some(pid) = child.id() else {
            return;
        };

        if let Err(e) = Signal::terminate(pid as i32) {
            tracing::debug!(unit = %self.spec.name, error = %e, "could not signal unit; killing");
            let _ = child.kill().await;
            return;
        }

        match tokio::time::timeout(Self::GRACE, child.wait()).await {
            Ok(_) => tracing::debug!(unit = %self.spec.name, "unit exited on request"),
            Err(_) => {
                tracing::warn!(unit = %self.spec.name, "unit ignored SIGTERM; killing");
                let _ = child.kill().await;
            }
        }
    }
}

impl Drop for UnitProcess {
    fn drop(&mut self) {
        self.supervisor.units.revoke(&self.spec.name);
        self.report(Transition::Stopped);
        self.supervisor.units.release(&self.spec.name);
    }
}
