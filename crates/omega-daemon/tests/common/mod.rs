// Shared by several test binaries: each uses a different part of it, and
// `pub` here is how they reach it at all.
#![allow(dead_code, unreachable_pub)]

use std::path::PathBuf;

use omega_core::UnitName;
use omega_daemon::Shutdown;
use omega_daemon::hub::Hub;
use omega_daemon::manifest::ManifestStore;
use omega_daemon::session::{Liveness, Session};
use omega_daemon::supervisor::Supervisor;
use omega_daemon::units::{UnitTable, UnitToken};
use omega_manifest::{Manifest, Surface};
use omega_wire::omega::{Frame, SurfaceKind};
use omega_wire::{Handshake, Socket, Transport};

/// An in-process daemon core: serves sessions over socket pairs, the same way
/// the real daemon serves them over its listener.
///
/// There is no listener and no socket file. `UnixStream::pair` is the whole
/// connection, and `SO_PEERCRED` reports this process on both ends — which is
/// precisely what a unit spawned by this process, or the operator who owns it,
/// presents to the daemon.
pub struct Harness {
    pub supervisor: Supervisor,
    pub hub: Hub,
    pub units: UnitTable,
    /// A keepalive cadence for sessions this harness serves, when a test needs
    /// one shorter than a desktop's.
    liveness: Option<Liveness>,
    /// The socket path spawned units are pointed at. Never bound: a test's
    /// units connect through the pair, not through the filesystem.
    socket: Socket,
}

impl Harness {
    pub fn new(tag: &str, manifests: ManifestStore) -> Self {
        let socket = TempSocket::new(tag).socket();
        let hub = Hub::new();
        let units = UnitTable::detached(hub.clone());
        units.adopt(&manifests);
        let supervisor = Supervisor::new(socket.clone(), units.clone(), Shutdown::new());

        Self {
            supervisor,
            hub,
            units,
            liveness: None,
            socket,
        }
    }

    /// Serve sessions with a shorter keepalive than the default.
    pub fn with_liveness(mut self, liveness: Liveness) -> Self {
        self.liveness = Some(liveness);
        self
    }

    pub fn socket(&self) -> &Socket {
        &self.socket
    }

    /// Register this process as a unit and take its token, the way a spawned
    /// unit receives one in its environment.
    pub fn register_unit(&self, name: &str) -> UnitToken {
        self.supervisor.register(&unit_name(name))
    }

    /// Open a connection and send `Hello`, returning the transport for the
    /// caller to read the daemon's answer from.
    pub async fn connect(
        &self,
        manifest_hash: &str,
        token: &str,
    ) -> Transport<tokio::net::UnixStream> {
        let (peer, daemon) = tokio::net::UnixStream::pair().unwrap();

        let mut session =
            Session::new(self.supervisor.clone(), self.hub.clone()).with_units(self.units.clone());
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

pub fn surface_id(id: &str) -> omega_core::SurfaceId {
    omega_core::SurfaceId::parse(id).unwrap()
}

pub fn unit_name(name: &str) -> UnitName {
    UnitName::parse(name).unwrap()
}

/// A unit that declares one widget surface and reads the battery topic — the
/// shape of every unit in the first vertical slice.
pub fn widget_manifest(name: &str, surface: &str) -> Manifest {
    Manifest {
        capabilities: vec!["CAPABILITY_STATE_READ".into()],
        surfaces: vec![Surface::new(surface_id(surface), SurfaceKind::Widget)],
        state_topics: vec!["battery".into()],
        ..Manifest::new(unit_name(name), "0.1.0")
    }
}

/// A unit that handles power events and may run commands — the shape of a
/// policy unit.
pub fn policy_manifest(name: &str) -> Manifest {
    Manifest {
        capabilities: vec!["CAPABILITY_STATE_READ".into(), "CAPABILITY_SPAWN".into()],
        events: vec![
            "EVENT_AC_PLUGGED".into(),
            "EVENT_AC_UNPLUGGED".into(),
            "EVENT_CUSTOM".into(),
        ],
        ..widget_manifest(name, "battery")
    }
}

/// A unit that declares a command surface — something to be asked to do.
pub fn command_manifest(name: &str, command: &str) -> Manifest {
    Manifest {
        capabilities: vec!["CAPABILITY_STATE_READ".into()],
        surfaces: vec![Surface::new(surface_id(command), SurfaceKind::Command)],
        state_topics: vec!["battery".into()],
        ..Manifest::new(unit_name(name), "0.1.0")
    }
}

/// The same unit, plus the capability to write its own keyspace.
pub fn writer_manifest(name: &str) -> Manifest {
    Manifest {
        capabilities: vec![
            "CAPABILITY_STATE_READ".into(),
            "CAPABILITY_STATE_WRITE".into(),
        ],
        ..widget_manifest(name, "battery")
    }
}

/// The next `Result` frame, skipping the state patches that arrive alongside
/// it — a unit's own keyspace write is replicated back to it, and the two
/// have no ordering guarantee.
pub async fn next_result(transport: &mut Transport<tokio::net::UnixStream>) -> Option<Frame> {
    loop {
        let frame = tokio::time::timeout(std::time::Duration::from_secs(2), transport.recv())
            .await
            .expect("timed out waiting for a Result")
            .unwrap()?;

        if matches!(frame.body, Some(omega_wire::omega::frame::Body::Result(_))) {
            return Some(frame);
        }
    }
}

/// The outcome a `Result` frame carries, or a panic naming what came instead.
pub fn expect_outcome(frame: Option<Frame>) -> omega_wire::omega::result::Outcome {
    let frame = frame.expect("expected a Result, got EOF");
    match frame.body {
        Some(omega_wire::omega::frame::Body::Result(result)) => {
            result.outcome.expect("a Result carries an outcome")
        }
        other => panic!("expected a Result, got {other:?}"),
    }
}

/// Asserts the op succeeded with nothing to return.
pub fn expect_ok(frame: Option<Frame>) {
    match expect_outcome(frame) {
        omega_wire::omega::result::Outcome::Ok(_) => {}
        other => panic!("expected Ok, got {other:?}"),
    }
}

/// The refusal a frame carries, or a panic naming what came instead.
pub fn expect_refusal(frame: Option<Frame>) -> omega_wire::Refusal {
    let frame = frame.expect("expected a refusal, got EOF");
    omega_wire::Refusal::of(&frame).unwrap_or_else(|| panic!("expected a refusal, got {frame:?}"))
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

    pub fn layout(&self) -> omega_core::Layout {
        omega_core::Layout::at(
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
