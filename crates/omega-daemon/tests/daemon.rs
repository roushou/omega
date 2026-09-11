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
use omega_daemon::{Daemon, DaemonHandle};
use omega_document::{Document, DocumentFile, Units};
use omega_host::Layout;
use omega_host::StateConfig;
use omega_proto::UnitName;
use omega_proto::omega::{RestartUnit, StateDocument, invoke, result};
use omega_proto::{Client, Socket};

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

        std::fs::write(
            self.layout.state_unit_manifest(&name),
            widget_manifest(name.as_str(), "battery").canonical(),
        )
        .unwrap();

        self.layout
            .file::<StateConfig>(())
            .write(&StateConfig::new(&self.layout, [name.clone()]))
            .unwrap();

        name
    }

    fn declare(&self, document: StateDocument) {
        self.publish(DocumentFile::encode(&document).unwrap().as_bytes());
    }

    /// A document that cannot be parsed, as a half-written build would leave.
    fn corrupt(&self) {
        self.publish(b"{ not a document");
    }

    fn publish(&self, document: &[u8]) {
        let generation = omega_host::Generations::new(&self.layout).stage().unwrap();
        let config = self.layout.file::<StateConfig>(()).read().unwrap();
        for unit in &config.units {
            generation
                .files()
                .copy(&self.layout.state_unit_program(&unit.name), &unit.program)
                .unwrap();
            generation
                .files()
                .copy(&self.layout.state_unit_manifest(&unit.name), &unit.manifest)
                .unwrap();
        }
        generation
            .files()
            .file::<StateConfig>(StateConfig::FILE_NAME)
            .write(&config)
            .unwrap();
        generation
            .files()
            .write(DocumentFile::FILE_NAME, document)
            .unwrap();
        generation.commit().unwrap();
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

#[tokio::test]
async fn a_machine_nothing_has_been_built_for_yet_still_starts() {
    // `omega init` installs the service before anything is built, so the very
    // first start finds a state dir with no `units.toml` in it. Treating that
    // as fatal made the daemon exit 1, and systemd restart it until it hit the
    // start limit and gave up — permanently dead by the time the first
    // `omega build` wrote the file three minutes later.
    let machine = Machine::new("unbuilt");
    assert!(
        !machine.layout.state_units_toml().exists(),
        "the point of this test is the file being absent"
    );

    let (handle, running) = machine.start();
    assert!(
        handle.supervisor.running().is_empty(),
        "nothing was built, so nothing runs"
    );
    stop(&handle, running).await;
}

#[tokio::test]
async fn activation_replaces_changed_binaries_and_preserves_identical_ones() {
    let machine = Machine::new("generation-replacement");
    let started = machine.tmp.path().join("started");
    let program = format!(
        "#!/bin/sh\nprintf 'first\\n' >> '{}'\nexec sleep 30\n",
        started.display()
    );
    machine.install("sleeper", &program);
    machine.declare(Document::new().env("ACTIVATED", "first").into_inner());
    let first = omega_host::Generations::new(&machine.layout)
        .pin_current()
        .unwrap()
        .unwrap()
        .layout()
        .clone();
    let (handle, running) = machine.start();
    assert!(
        until(Duration::from_secs(5), || std::fs::read_to_string(&started)
            .is_ok_and(|value| value == "first\n"))
        .await
    );

    machine.declare(Document::new().env("ACTIVATED", "identical").into_inner());
    let environment = machine.layout.state.join("environment");
    assert!(
        until(Duration::from_secs(5), || std::fs::read_to_string(
            &environment
        )
        .is_ok_and(|value| value.contains("identical")))
        .await
    );
    assert_eq!(std::fs::read_to_string(&started).unwrap(), "first\n");

    let program = format!(
        "#!/bin/sh\nprintf 'second\\n' >> '{}'\nexec sleep 30\n",
        started.display()
    );
    machine.install("sleeper", &program);
    machine.declare(Document::new().env("ACTIVATED", "changed").into_inner());
    assert!(
        until(Duration::from_secs(8), || std::fs::read_to_string(&started)
            .is_ok_and(|value| value == "first\nsecond\n"))
        .await
    );
    assert!(
        std::fs::read_to_string(first.state_unit_program(&UnitName::parse("sleeper").unwrap()))
            .unwrap()
            .contains("first")
    );
    stop(&handle, running).await;
}

#[tokio::test]
async fn editing_and_removing_settings_replace_the_process() {
    let machine = Machine::new("generation-settings");
    let started = machine.tmp.path().join("started");
    let program = format!(
        "#!/bin/sh\nprintf 'started\\n' >> '{}'\nexec sleep 30\n",
        started.display()
    );
    let name = machine.install("sleeper", &program);
    machine.declare(Document::new().into_inner());
    let (handle, running) = machine.start();
    assert!(
        until(Duration::from_secs(5), || std::fs::read_to_string(&started)
            .is_ok_and(|value| value.lines().count() == 1))
        .await
    );
    machine.declare(
        Document::new()
            .unit(Units::configured(
                "sleeper",
                &omega_proto::Values::new().with("label", "changed"),
            ))
            .into_inner(),
    );
    assert!(
        until(Duration::from_secs(8), || std::fs::read_to_string(&started)
            .is_ok_and(|value| value.lines().count() == 2))
        .await
    );
    assert_eq!(
        omega_proto::Values::from_map(handle.units.config(&name))
            .get::<String>("label")
            .as_deref(),
        Some("changed")
    );
    machine.declare(Document::new().into_inner());
    assert!(
        until(Duration::from_secs(8), || std::fs::read_to_string(&started)
            .is_ok_and(|value| value.lines().count() == 3))
        .await
    );
    assert!(handle.units.config(&name).is_empty());
    stop(&handle, running).await;
}

#[tokio::test]
async fn restart_recovers_accepted_and_then_previous_when_candidates_are_invalid() {
    let machine = Machine::new("durable-recovery");
    machine.install("sleeper", "#!/bin/sh\nexec sleep 30\n");
    let store = omega_host::Generations::new(&machine.layout);
    let environment = machine.layout.state.join("environment");
    machine.declare(Document::new().env("RECOVERY", "first").into_inner());
    let first = store.pin_current().unwrap().unwrap().layout().clone();
    let (handle, running) = machine.start();
    assert!(
        until(Duration::from_secs(5), || std::fs::read_to_string(
            &environment
        )
        .is_ok_and(|value| value.contains("first")))
        .await
    );
    machine.declare(Document::new().env("RECOVERY", "second").into_inner());
    let second = store.pin_current().unwrap().unwrap().layout().clone();
    assert!(
        until(Duration::from_secs(5), || std::fs::read_to_string(
            &environment
        )
        .is_ok_and(|value| value.contains("second")))
        .await
    );
    stop(&handle, running).await;

    machine.corrupt();
    let invalid = std::fs::read(machine.layout.active_build()).unwrap();
    std::fs::remove_file(&environment).unwrap();
    let (handle, running) = machine.start();
    assert!(
        until(Duration::from_secs(5), || std::fs::read_to_string(
            &environment
        )
        .is_ok_and(|value| value.contains("second")))
        .await
    );
    assert_eq!(
        std::fs::read(machine.layout.active_build()).unwrap(),
        invalid
    );
    stop(&handle, running).await;

    omega_host::AtomicFile::at(second.state.join(DocumentFile::FILE_NAME))
        .write(b"invalid")
        .unwrap();
    std::fs::remove_file(&environment).unwrap();
    let (handle, running) = machine.start();
    assert!(
        until(Duration::from_secs(5), || std::fs::read_to_string(
            &environment
        )
        .is_ok_and(|value| value.contains("first")))
        .await
    );
    assert_eq!(
        store
            .pin(&store.recovery_ids().unwrap()[0])
            .unwrap()
            .layout()
            .state,
        first.state
    );
    stop(&handle, running).await;
}

#[tokio::test]
async fn cleanup_keeps_an_unchanged_units_executable_across_multiple_activations() {
    let machine = Machine::new("live-generation-cleanup");
    let started = machine.tmp.path().join("starts");
    let unit = machine.install(
        "sleeper",
        &format!(
            "#!/bin/sh\nprintf 'start\\n' >> '{}'\nexec sleep 30\n",
            started.display()
        ),
    );
    let store = omega_host::Generations::new(&machine.layout);
    machine.declare(Document::new().into_inner());
    let first = store.pin_current().unwrap().unwrap().layout().clone();
    let (handle, running) = machine.start();
    assert!(until(Duration::from_secs(5), || started.exists()).await);
    let environment = machine.layout.state.join("environment");
    for value in ["second", "third"] {
        machine.declare(Document::new().env("GENERATION", value).into_inner());
        assert!(
            until(Duration::from_secs(5), || std::fs::read_to_string(
                &environment
            )
            .is_ok_and(|contents| contents.contains(value)))
            .await
        );
    }
    assert!(store.clean().unwrap().is_empty());
    assert!(first.state.exists());
    assert!(handle.supervisor.restart(&unit));
    assert!(
        until(Duration::from_secs(5), || std::fs::read_to_string(&started)
            .is_ok_and(|contents| contents == "start\nstart\n"))
        .await
    );
    stop(&handle, running).await;
    assert_eq!(store.clean().unwrap().len(), 1);
    assert!(!first.state.exists());
}

#[tokio::test(start_paused = true)]
async fn daemon_shutdown_joins_control_and_observation_connections() {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    let machine = Machine::new("joined-connections");
    let (handle, running) = machine.start();
    let (mut control, _) = Client::connect(handle.control(), "", "").await.unwrap();
    let mut observer = BufReader::new(handle.observation().connect_stream().await.unwrap());
    observer.get_mut().write_all(b"{}\n").await.unwrap();
    loop {
        let mut line = String::new();
        tokio::time::timeout(Duration::from_secs(1), observer.read_line(&mut line))
            .await
            .unwrap()
            .unwrap();
        if omega_proto::Observation::answer(&line).is_some() {
            break;
        }
    }
    // The accept futures lose this select race; their connections must survive it.
    tokio::time::advance(Duration::from_secs(2)).await;
    let stream = control.allocate();
    control
        .invoke(stream, invoke::Op::GetState(Default::default()))
        .await
        .unwrap();
    assert!(matches!(
        control.answer(stream).await,
        Err(omega_proto::ClientError::Refused(_))
    ));
    // Socket EOF readiness follows the OS, not Tokio's virtual clock.
    tokio::time::resume();
    stop(&handle, running).await;
    assert!(
        tokio::time::timeout(Duration::from_secs(1), control.recv())
            .await
            .unwrap()
            .unwrap()
            .is_none()
    );
    loop {
        let mut line = String::new();
        if tokio::time::timeout(Duration::from_secs(1), observer.read_line(&mut line))
            .await
            .unwrap()
            .unwrap()
            == 0
        {
            break;
        }
    }
}

struct PanickingBroker;
#[async_trait::async_trait]
impl omega_brokers::Broker for PanickingBroker {
    fn name(&self) -> &'static str {
        "panicking-test"
    }
    fn topics(&self) -> &'static [omega_proto::SystemTopic] {
        &[]
    }
    async fn connect(&mut self) -> Result<(), omega_brokers::BrokerError> {
        panic!("test broker invariant");
    }
}

#[tokio::test]
async fn a_background_task_panic_stops_the_daemon_with_its_identity() {
    let machine = Machine::new("background-failure");
    let daemon = Daemon::builder(&machine.layout)
        .control(Socket::at(machine.tmp.path().join("control.sock")))
        .observation(Socket::at(machine.tmp.path().join("observe.sock")))
        .build()
        .unwrap();
    daemon.add_broker(Box::new(PanickingBroker));
    let result = tokio::time::timeout(Duration::from_secs(2), daemon.run())
        .await
        .unwrap();
    let error = result.unwrap_err().to_string();
    assert!(error.contains("broker panicking-test"), "{error}");
    assert!(error.contains("test broker invariant"), "{error}");
}

#[tokio::test]
async fn failed_observation_bind_releases_the_control_endpoint() {
    let machine = Machine::new("partial-bind");
    let control = Socket::at(machine.layout.state.join("control.sock"));
    let observation = Socket::at(machine.layout.state.join("observation.sock"));
    std::fs::write(observation.path(), b"keep").unwrap();
    assert!(
        Daemon::builder(&machine.layout)
            .control(control.clone())
            .observation(observation.clone())
            .build()
            .is_err()
    );
    assert!(!control.path().exists());
    assert_eq!(std::fs::read(observation.path()).unwrap(), b"keep");
}
