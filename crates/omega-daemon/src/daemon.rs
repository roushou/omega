//! The daemon core: binds the sockets and ties brokers, units, sessions, and
//! the shell together.

use std::io;
use std::time::Duration;

use tokio::signal::unix::SignalKind;

use omega_host::Layout;
use omega_proto::{Observation, Socket};

use crate::broker::Brokerage;
use crate::hub::Hub;
use crate::manifest::ManifestStoreError;
use crate::reconcile::{Context, Converger, Work};
use crate::schedule::Schedules;
use crate::session::Session;
use crate::shell::ShellError;
use crate::shell::ShellServer;
use crate::shutdown::Shutdown;
use crate::supervisor::Supervisor;
use crate::units::UnitTable;
use crate::watch::StateStamp;
use omega_host::TomlError;
use std::path::PathBuf;

/// The Omega daemon: trust boundary, state owner, supervisor.
#[derive(Debug)]
pub struct Daemon {
    hub: Hub,
    /// The subsystems this daemon brokers.
    brokers: Brokerage,
    supervisor: Supervisor,
    listener: omega_proto::BoundSocket,
    socket: Socket,
    shell: ShellServer,
    deployment: crate::reconcile::deployment::Deployment,
    shutdown: Shutdown,
    /// Everything known about the units, including how to reach them.
    units: UnitTable,
    /// Units announcing themselves. Taken by `run`.
    arrivals: std::sync::Mutex<Option<tokio::sync::mpsc::Receiver<omega_proto::UnitName>>>,
    /// Where the built state lives, so a rebuild can be picked up without a
    /// restart.
    layout: Layout,
}

impl Daemon {
    /// How long the daemon waits for its units to exit before giving up on
    /// an orderly stop.
    const SHUTDOWN_GRACE: Duration = Duration::from_secs(8);
    /// How long to let a build's writes settle before adopting it.
    const SETTLE: Duration = Duration::from_millis(250);

    /// Build the daemon from the filesystem layout, on the sockets the
    /// environment names.
    pub fn from_layout(layout: &Layout) -> Result<Self, DaemonError> {
        Self::builder(layout).build()
    }

    /// Build the daemon with its endpoints given rather than resolved.
    ///
    /// The two socket paths are the only things the daemon cannot derive from
    /// the layout, and resolving them from the environment is what confines a
    /// daemon to one per process. Naming them is what lets several run at
    /// once, against temp dirs, in a test.
    pub fn builder(layout: &Layout) -> DaemonBuilder {
        DaemonBuilder {
            layout: layout.clone(),
            control: None,
            observation: None,
        }
    }

    /// The parts a caller outside the run loop can watch and stop it with.
    pub fn handle(&self) -> DaemonHandle {
        DaemonHandle {
            hub: self.hub.clone(),
            units: self.units.clone(),
            supervisor: self.supervisor.clone(),
            shutdown: self.shutdown.clone(),
            control: self.socket.clone(),
            observation: Socket::at(self.shell.path()),
        }
    }

    /// Start a broker: its topics are published into the hub, and it stops
    /// when the daemon does.
    pub fn add_broker(&self, broker: Box<dyn omega_brokers::Broker>) {
        self.brokers.add(broker);
    }

    /// Run until interrupted, then shut down in order: stop accepting, ask
    /// every unit to exit, and wait for them.
    pub async fn run(self) -> Result<(), DaemonError> {
        tracing::info!(path = %self.socket.path().display(), "daemon listening");
        tracing::info!(shell = %self.shell.path().display(), "shell socket listening");

        // Convergence runs in its own task: a bar instance can wait five
        // seconds on a wedged unit, and the daemon must keep accepting
        // connections and answering signals while it does.
        let schedules = Schedules::new(
            self.hub.clone(),
            self.units.clone(),
            self.brokers.clone(),
            self.shutdown.clone(),
        );
        let converger = Converger::spawn(
            Context {
                deployment: self.deployment.clone(),
                layout: self.layout.clone(),
                supervisor: self.supervisor.clone(),
                units: self.units.clone(),
                schedules: schedules.clone(),
            },
            self.shutdown.clone(),
        );

        let mut arrivals = self
            .arrivals
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take();
        let mut built = StateStamp::of(&self.layout);
        // Watching the parent also covers a state directory created after startup.
        let mut rebuilt = StateStamp::watch(&self.layout, Self::SETTLE)
            .inspect_err(|e| tracing::warn!(error = %e, "cannot watch the state dir for rebuilds"))
            .ok();

        let mut recovery = tokio::time::interval(Duration::from_secs(2));
        recovery.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let mut sessions = tokio::task::JoinSet::new();
        let mut observers = tokio::task::JoinSet::new();
        let outcome = loop {
            tokio::select! {
                // Both loops are cancel-safe: a pending `accept` that loses
                // the race is simply started again on the next pass.
                result = self.accept_loop(&mut sessions) => break result.map_err(DaemonError::from),
                result = self.shell.accepting(&mut observers) => break result.map_err(DaemonError::from),
                _ = recovery.tick() => {
                    if built.changed(&self.layout) {
                        built = StateStamp::of(&self.layout);
                        converger.request(Work::REBUILD);
                    }
                }
                Some(()) = Self::rebuilt(rebuilt.as_mut()) => {
                    // The watch says something moved; the stamp says whether
                    // it was the pair of files this daemon runs from.
                    if !built.changed(&self.layout) {
                        continue;
                    }
                    built = StateStamp::of(&self.layout);
                    tracing::info!("the config was rebuilt; reconciling");
                    converger.request(Work::REBUILD);
                }
                // A unit that just connected can be asked for the instances
                // the document gave it, which it could not be a moment ago.
                Some(unit) = Self::arrived(arrivals.as_mut()) => {
                    tracing::debug!(%unit, "unit connected; reconciling");
                    converger.request(Work::CONVERGE);
                }
                signal = Self::interrupted() => {
                    tracing::info!(%signal, "shutting down");
                    break Ok(());
                }
                // The same stop a signal asks for, asked for in-process.
                _ = self.shutdown.wait() => {
                    tracing::info!("stop requested; shutting down");
                    break Ok(());
                }
            }
        };

        self.shutdown.trigger();
        converger.stop().await;
        schedules.shutdown().await;
        sessions.shutdown().await;
        observers.shutdown().await;
        self.stop().await;
        if let Some(error) = self.shutdown.failure() {
            return Err(DaemonError::Task(error));
        }
        outcome
    }

    /// The next settled change to the state dir, or never when it could not
    /// be watched.
    async fn rebuilt(changes: Option<&mut crate::host::Changes>) -> Option<()> {
        match changes {
            Some(changes) => changes.next().await,
            None => std::future::pending().await,
        }
    }

    /// The next unit to connect, or never when the daemon is not tracking
    /// arrivals (a test's in-process core, say).
    async fn arrived(
        arrivals: Option<&mut tokio::sync::mpsc::Receiver<omega_proto::UnitName>>,
    ) -> Option<omega_proto::UnitName> {
        match arrivals {
            Some(arrivals) => arrivals.recv().await,
            None => std::future::pending().await,
        }
    }

    /// The first signal asking the daemon to stop.
    async fn interrupted() -> &'static str {
        let mut terminate = match tokio::signal::unix::signal(SignalKind::terminate()) {
            Ok(stream) => stream,
            // Without SIGTERM there is still Ctrl-C; a daemon that cannot
            // listen for one signal should not refuse to run.
            Err(e) => {
                tracing::warn!(error = %e, "cannot listen for SIGTERM");
                let _ = tokio::signal::ctrl_c().await;
                return "SIGINT";
            }
        };

        tokio::select! {
            _ = tokio::signal::ctrl_c() => "SIGINT",
            _ = terminate.recv() => "SIGTERM",
        }
    }

    /// Tell everything to wind down and give the units time to do it. The
    /// supervisor's tasks send SIGTERM and wait; this bounds how long the
    /// daemon waits for them.
    async fn stop(&self) {
        self.shutdown.trigger();
        self.brokers.stop().await;

        let deadline = tokio::time::Instant::now() + Self::SHUTDOWN_GRACE;
        while tokio::time::Instant::now() < deadline {
            if self.supervisor.all_stopped() {
                tracing::info!("every unit stopped");
                return;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        tracing::warn!("shutdown deadline reached; leaving remaining units to the kernel");
    }

    const CONNECTION_LIMIT: usize = 64;

    async fn accept_loop(&self, sessions: &mut tokio::task::JoinSet<()>) -> io::Result<()> {
        loop {
            tokio::select! {
                Some(result) = sessions.join_next(), if !sessions.is_empty() => {
                    if let Err(error) = result { tracing::warn!(%error, "control session task failed"); }
                }
                accepted = self.listener.accept(), if sessions.len() < Self::CONNECTION_LIMIT => {
                    let (stream, _) = accepted?;
                    let session = Session::new(self.supervisor.clone(), self.hub.clone())
                        .with_layout(self.layout.clone()).with_deployment(self.deployment.clone())
                        .with_brokers(self.brokers.clone())
                        .with_shutdown(self.shutdown.clone())
                        .with_units(self.units.clone());
                    sessions.spawn(async move {
                        if let Err(error) = session.serve(stream).await {
                            tracing::warn!(%error, "connection ended");
                        }
                    });
                }
            }
        }
    }
}

/// A daemon under construction: the layout, plus the endpoints it is reached
/// on. Both sockets default to the paths the environment names.
#[derive(Debug)]
pub struct DaemonBuilder {
    layout: Layout,
    control: Option<Socket>,
    observation: Option<Socket>,
}

impl DaemonBuilder {
    /// Where units and the operator connect.
    pub fn control(mut self, socket: Socket) -> Self {
        self.control = Some(socket);
        self
    }

    /// Where the shell reads views and topics.
    pub fn observation(mut self, socket: Socket) -> Self {
        self.observation = Some(socket);
        self
    }

    /// Bind both sockets and load what the state dir declares.
    pub fn build(self) -> Result<Daemon, DaemonError> {
        let socket = self.control.unwrap_or_else(Socket::resolve);
        let listener = socket.bind().map_err(|source| DaemonError::Bind {
            path: socket.path().to_path_buf(),
            source,
        })?;

        let hub = Hub::new();
        let shutdown = Shutdown::new();
        let brokers = Brokerage::new(hub.clone(), shutdown.clone());
        let (units, arrivals) = UnitTable::new(hub.clone());
        let supervisor = Supervisor::new(socket.clone(), units.clone(), shutdown.clone());
        let deployment = crate::reconcile::deployment::Deployment::default();
        let shell = ShellServer::bind_at(
            self.observation.unwrap_or_else(Observation::socket),
            hub.clone(),
        )?
        // A shell draws what a plugin publishes, so it has to be able to
        // press what it drew.
        .serving(supervisor.clone(), units.clone(), brokers.clone())
        .with_layout(self.layout.clone())
        .with_deployment(deployment.clone());

        Ok(Daemon {
            brokers,
            hub,
            supervisor,
            listener,
            socket,
            shell,
            deployment,
            shutdown,
            units,
            arrivals: std::sync::Mutex::new(Some(arrivals)),
            layout: self.layout,
        })
    }
}

/// A running daemon, seen from outside its run loop.
///
/// Cloneable and inert: it holds the same state the daemon serves from, so a
/// caller can watch what the daemon knows and ask it to stop, without being
/// the loop.
#[derive(Debug, Clone)]
pub struct DaemonHandle {
    pub hub: Hub,
    pub units: UnitTable,
    pub supervisor: Supervisor,
    shutdown: Shutdown,
    control: Socket,
    observation: Socket,
}

impl DaemonHandle {
    pub fn control(&self) -> &Socket {
        &self.control
    }

    pub fn observation(&self) -> &Socket {
        &self.observation
    }

    /// Ask the daemon to stop, exactly as a signal would.
    pub fn stop(&self) {
        self.shutdown.trigger();
    }

    pub fn is_stopping(&self) -> bool {
        self.shutdown.is_triggered()
    }
}

/// What starting or running the daemon can fail with.
#[derive(Debug, thiserror::Error)]
pub enum DaemonError {
    #[error(transparent)]
    Task(#[from] crate::shutdown::TaskFailure),
    #[error("invalid desired state: {0}")]
    DesiredState(#[from] crate::reconcile::ProviderError),
    #[error("cannot bind control socket {}: {source}", path.display())]
    Bind {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("cannot load the state config: {0}")]
    Config(#[from] TomlError),
    #[error("cannot load unit manifests: {0}")]
    Manifests(#[from] ManifestStoreError),
    #[error("cannot load the state document: {0}")]
    Document(#[from] omega_document::DocumentError),
    #[error("{0}")]
    Shell(#[from] ShellError),
    #[error("io error: {0}")]
    Io(#[from] io::Error),
}
