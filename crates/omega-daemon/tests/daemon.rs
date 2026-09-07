//! The daemon itself: the run loop, against a state dir on disk.
//!
//! Everything below drives a real `Daemon` — its listener, its watch on the
//! state dir, its converger, its supervisor — and asserts on what the daemon
//! knows afterwards. The only things given to it are the two socket paths and
//! a temp directory to be a machine.

mod common;

use std::os::unix::fs::PermissionsExt;
use std::time::Duration;

use common::{TempDir, widget_manifest};
use omega_daemon::host::StateConfig;
use omega_daemon::{Daemon, DaemonHandle};
use omega_document::{Document, DocumentFile, Units};
use omega_proto::Manifest;
use omega_proto::omega::{RestartUnit, StateDocument, invoke, result};
use omega_proto::{Client, Socket};
use omega_proto::{Layout, UnitName};

/// A machine as `omega build` leaves it: a state dir holding a document, the
/// config of what was built, and one directory per unit.
struct Machine {
    tmp: TempDir,
    layout: Layout,
}

impl Machine {
    fn new(tag: &str) -> Self {
        let tmp = TempDir::new(tag);
        let layout = tmp.layout();
        std::fs::create_dir_all(&layout.state).unwrap();
        Self { tmp, layout }
    }

    /// Install a unit the way a build does: a binary, a canonical manifest,
    /// and an entry in `units.toml`.
    fn install(&self, name: &str, program: &str) -> UnitName {
        let name = UnitName::parse(name).unwrap();

        let binary = self.layout.state_unit_program(&name);
        std::fs::create_dir_all(binary.parent().unwrap()).unwrap();
        std::fs::write(&binary, program).unwrap();
        std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o755)).unwrap();

        self.layout
            .file::<Manifest>(&name)
            .write(&widget_manifest(name.as_str(), "battery"))
            .unwrap();

        self.layout
            .file::<StateConfig>(())
            .write(&StateConfig::new(&self.layout, [name.clone()]))
            .unwrap();

        name
    }

    fn declare(&self, document: StateDocument) {
        DocumentFile::of(&self.layout).write(&document).unwrap();
    }

    /// A document that cannot be parsed, as a half-written build would leave.
    fn corrupt(&self) {
        std::fs::write(DocumentFile::of(&self.layout).path(), "{ not a document").unwrap();
    }

    /// Start the daemon on sockets of this machine's own, and hand back the
    /// handle plus the task running the loop.
    fn start(&self) -> (DaemonHandle, tokio::task::JoinHandle<()>) {
        let daemon = Daemon::builder(&self.layout)
            .control(Socket::at(self.tmp.path().join("omega.sock")))
            .observation(Socket::at(self.tmp.path().join("shell.sock")))
            .build()
            .expect("the daemon must start against a built state dir");

        let handle = daemon.handle();
        let running = tokio::spawn(async move {
            daemon.run().await.expect("the daemon must stop cleanly");
        });
        (handle, running)
    }
}

/// A unit that stays up until something stops it.
const SLEEPER: &str = "#!/bin/sh\nsleep 30\n";

/// Poll until `done`, or give up. Returns whether it happened.
async fn until(within: Duration, mut done: impl FnMut() -> bool) -> bool {
    let deadline = tokio::time::Instant::now() + within;
    while tokio::time::Instant::now() < deadline {
        if done() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    done()
}

/// Stop the daemon and wait for the loop to finish, so the assertions that
/// follow are about a daemon that has actually stopped.
async fn stop(handle: &DaemonHandle, running: tokio::task::JoinHandle<()>) {
    handle.stop();
    tokio::time::timeout(Duration::from_secs(10), running)
        .await
        .expect("the run loop must return when asked to stop")
        .unwrap();
}

#[tokio::test]
async fn a_running_daemon_serves_its_owner_and_stops_its_units() {
    let machine = Machine::new("daemon-serve");
    let sleeper = machine.install("sleeper", SLEEPER);
    machine.declare(Document::new().into_inner());

    let (handle, running) = machine.start();

    // A built unit the document never mentions runs, and the daemon reports
    // it through the same table a session reads.
    assert!(
        until(Duration::from_secs(5), || handle
            .supervisor
            .running()
            .contains(&sleeper))
        .await,
        "the daemon must start what the build declared"
    );

    // The operator is admitted on the uid alone, and lifecycle is what it may
    // ask for.
    let (mut client, welcome) = Client::connect(handle.control(), "", "")
        .await
        .expect("the daemon must serve its owner");
    assert!(welcome.unit_id.starts_with("operator-"), "{welcome:?}");

    let stream = client.allocate();
    client
        .invoke(
            stream,
            invoke::Op::RestartUnit(RestartUnit {
                unit: sleeper.to_string(),
            }),
        )
        .await
        .unwrap();
    assert!(matches!(
        client.answer(stream).await.unwrap(),
        result::Outcome::Ok(_)
    ));

    stop(&handle, running).await;
    assert!(
        handle.supervisor.all_stopped(),
        "a daemon that has stopped is not still running units"
    );
}

#[tokio::test]
async fn a_rebuilt_document_is_adopted_without_a_restart() {
    let machine = Machine::new("daemon-rebuild");
    let sleeper = machine.install("sleeper", SLEEPER);
    machine.declare(
        Document::new()
            .unit(Units::disabled("sleeper"))
            .into_inner(),
    );

    let (handle, running) = machine.start();

    assert!(
        !until(Duration::from_millis(500), || !handle
            .supervisor
            .running()
            .is_empty())
        .await,
        "a document that disables a unit must not start it"
    );

    // The build lands under a running daemon.
    machine.declare(Document::new().unit(Units::enabled("sleeper")).into_inner());

    assert!(
        until(Duration::from_secs(5), || handle
            .supervisor
            .running()
            .contains(&sleeper))
        .await,
        "a rebuilt document must be picked up without restarting the daemon"
    );

    stop(&handle, running).await;
}

#[tokio::test]
async fn a_broken_build_leaves_the_running_one_alone() {
    let machine = Machine::new("daemon-broken");
    let sleeper = machine.install("sleeper", SLEEPER);
    machine.declare(Document::new().into_inner());

    let (handle, running) = machine.start();
    assert!(
        until(Duration::from_secs(5), || handle
            .supervisor
            .running()
            .contains(&sleeper))
        .await
    );

    machine.corrupt();

    // The daemon noticed, could not adopt it, and kept what it was running.
    tokio::time::sleep(Duration::from_secs(1)).await;
    assert!(
        handle.supervisor.running().contains(&sleeper),
        "a build that cannot be read must not take down the last good one"
    );
    Client::connect(handle.control(), "", "")
        .await
        .expect("the daemon must still be serving");

    stop(&handle, running).await;
}
