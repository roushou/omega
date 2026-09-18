// Shared by several test binaries: each uses a different part of it, and
// `pub` here is how they reach it at all.
#![allow(dead_code, unreachable_pub)]

use std::path::PathBuf;

use omega_daemon::Shutdown;
use omega_daemon::broker::Brokerage;
use omega_daemon::hub::Hub;
use omega_daemon::manifest::ManifestStore;
use omega_daemon::plugins::{PluginRegistry, PluginToken};
use omega_daemon::session::{Liveness, Session};
use omega_daemon::supervisor::Supervisor;
use omega_proto::PluginName;
use omega_proto::omega::{Capability, EventKind, Frame, SurfaceKind};
use omega_proto::{Handshake, Socket, Transport};
use omega_proto::{Manifest, Surface};

/// In-process sessions over UnixStream::pair; peer credentials identify the test process.
pub struct Harness {
    pub supervisor: Supervisor,
    pub hub: Hub,
    pub plugins: PluginRegistry,
    /// A keepalive cadence for sessions this harness serves, when a test needs
    /// one shorter than a desktop's.
    liveness: Option<Liveness>,
    /// The socket path spawned plugins are pointed at. Never bound: a test's
    /// plugins connect through the pair, not through the filesystem.
    socket: Socket,
    /// Empty unless a test registers one, so an action nothing claims is
    /// refused exactly as it is on a daemon with no brokers.
    brokers: Brokerage,
}

impl Harness {
    pub fn new(tag: &str, manifests: ManifestStore) -> Self {
        let socket = TempSocket::new(tag).socket();
        let hub = Hub::new();
        let plugins = PluginRegistry::detached(hub.clone());
        plugins.adopt(&manifests);
        let supervisor = Supervisor::new(socket.clone(), plugins.clone(), Shutdown::new());

        Self {
            supervisor,
            brokers: Brokerage::new(hub.clone(), Shutdown::new()),
            hub,
            plugins,
            liveness: None,
            socket,
        }
    }

    /// Register a broker, so an action it claims can reach it.
    pub fn with_broker(self, broker: Box<dyn omega_platform::Broker>) -> Self {
        self.brokers.add(broker);
        self
    }

    /// Serve sessions with a shorter keepalive than the default.
    pub fn with_liveness(mut self, liveness: Liveness) -> Self {
        self.liveness = Some(liveness);
        self
    }

    pub fn socket(&self) -> &Socket {
        &self.socket
    }

    /// Register this process as a plugin and take its token, the way a spawned
    /// plugin receives one in its environment.
    pub fn register_plugin(&self, name: &str) -> PluginToken {
        self.supervisor.register(&plugin_name(name)).unwrap()
    }

    /// Open a connection and send `Hello`, returning the transport for the
    /// caller to read the daemon's answer from.
    pub async fn connect(
        &self,
        manifest_hash: &str,
        token: &str,
    ) -> Transport<tokio::net::UnixStream> {
        let (peer, daemon) = tokio::net::UnixStream::pair().unwrap();

        let mut session = Session::new(self.supervisor.clone(), self.hub.clone())
            .with_plugins(self.plugins.clone())
            .with_brokers(self.brokers.clone());
        if let Some(liveness) = &self.liveness {
            session = session.with_liveness(liveness.clone());
        }
        tokio::spawn(async move {
            if let Err(e) = session.serve(daemon).await {
                tracing::warn!("test session ended: {e}");
            }
        });

        let mut transport = Transport::new(peer);
        transport
            .send(Handshake::hello(manifest_hash, token))
            .await
            .unwrap();
        transport
    }
}

pub fn surface_id(id: &str) -> omega_proto::SurfaceId {
    omega_proto::SurfaceId::try_from(id).unwrap()
}

pub fn plugin_name(name: &str) -> PluginName {
    PluginName::try_from(name).unwrap()
}

/// A plugin that declares one widget surface and reads the battery topic — the
/// shape of every plugin in the first vertical slice.
pub fn widget_manifest(name: &str, surface: &str) -> Manifest {
    Manifest::new(&plugin_name(name), "0.1.0")
        .granting([Capability::StateRead])
        .exposing([Surface::new(&surface_id(surface), SurfaceKind::Widget)])
        .reading(["battery"])
}

/// A plugin that handles power events and may run commands — the shape of a
/// policy plugin.
pub fn policy_manifest(name: &str) -> Manifest {
    widget_manifest(name, "battery")
        .granting([Capability::StateRead, Capability::Spawn])
        .handling([
            EventKind::EventAcPlugged,
            EventKind::EventAcUnplugged,
            EventKind::EventCustom,
        ])
}

/// A plugin that declares a command surface — something to be asked to do.
pub fn command_manifest(name: &str, command: &str) -> Manifest {
    Manifest::new(&plugin_name(name), "0.1.0")
        .granting([Capability::StateRead])
        .serving([omega_proto::omega::CommandEndpoint {
            input: Some(Default::default()),
            output: Some(Default::default()),
            description: String::new(),
            id: command.into(),
        }])
        .reading(["battery"])
}

/// The same plugin, plus the capability to write its own keyspace.
pub fn writer_manifest(name: &str) -> Manifest {
    widget_manifest(name, "battery").granting([Capability::StateRead, Capability::StateWrite])
}

/// Read the next result, skipping independently ordered state patches.
pub async fn next_result(transport: &mut Transport<tokio::net::UnixStream>) -> Option<Frame> {
    loop {
        let frame = tokio::time::timeout(std::time::Duration::from_secs(2), transport.recv())
            .await
            .expect("timed out waiting for a Result")
            .unwrap()?;

        if matches!(frame.body, Some(omega_proto::omega::frame::Body::Result(_))) {
            return Some(frame);
        }
    }
}

/// The outcome a `Result` frame carries, or a panic naming what came instead.
pub fn expect_outcome(frame: Option<Frame>) -> omega_proto::omega::result::Outcome {
    let frame = frame.expect("expected a Result, got EOF");
    match frame.body {
        Some(omega_proto::omega::frame::Body::Result(result)) => {
            result.outcome.expect("a Result carries an outcome")
        }
        other => panic!("expected a Result, got {other:?}"),
    }
}

/// Asserts the op succeeded with nothing to return.
pub fn expect_ok(frame: Option<Frame>) {
    match expect_outcome(frame) {
        omega_proto::omega::result::Outcome::Ok(_) => {}
        other => panic!("expected Ok, got {other:?}"),
    }
}

/// The refusal a frame carries, or a panic naming what came instead.
pub fn expect_refusal(frame: Option<Frame>) -> omega_proto::Refusal {
    let frame = frame.expect("expected a refusal, got EOF");
    omega_proto::Refusal::of(&frame).unwrap_or_else(|| panic!("expected a refusal, got {frame:?}"))
}

/// A temp directory holding one test's three roots, removed when it drops.
pub struct TempDir(PathBuf);

impl TempDir {
    pub fn new(tag: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("omega-{tag}-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    pub fn path(&self) -> &std::path::Path {
        &self.0
    }

    pub fn layout(&self) -> omega_host::Layout {
        omega_host::Layout::at(
            self.0.join("config"),
            self.0.join("state"),
            self.0.join("cache"),
        )
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A socket path that cleans up after itself, so a test run leaves no
/// sockets behind in the temp dir.
pub struct TempSocket(PathBuf);

impl TempSocket {
    pub fn new(tag: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        Self(std::env::temp_dir().join(format!("omega-{tag}-{}-{nanos}.sock", std::process::id())))
    }

    pub fn socket(&self) -> Socket {
        Socket::at(self.0.clone())
    }
}

impl Drop for TempSocket {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}
