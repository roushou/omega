//! Supervision: pacing restarts, reporting health, and stopping in order.

mod common;

use std::time::Duration;

use common::widget_manifest;
use omega_daemon::hub::Hub;
use omega_daemon::manifest::ManifestStore;
use omega_daemon::supervisor::{Backoff, Supervisor, UnitSpec};
use omega_daemon::units::{Transition, UnitTable};
use omega_daemon::{Shutdown, UnitToken};
use omega_proto::UnitName;
use omega_proto::omega::UnitPhase;
use omega_proto::{Socket, SystemTopic};

/// A table holding the manifests a test declares, which is what a supervisor
/// now needs instead of a manifest store of its own.
fn table_with(manifests: ManifestStore) -> UnitTable {
    let units = UnitTable::detached(Hub::new());
    units.adopt(&manifests);
    units
}

fn unit(name: &str) -> UnitName {
    UnitName::parse(name).unwrap()
}

#[test]
fn backoff_grows_and_is_capped() {
    let mut backoff = Backoff::with(Duration::from_millis(100), Duration::from_millis(800));

    let first = backoff.delay();
    let second = backoff.delay();
    let third = backoff.delay();

    assert!(second > first, "{second:?} should exceed {first:?}");
    assert!(third > second, "{third:?} should exceed {second:?}");

    // A unit failing forever waits the cap, not forever-doubling.
    for _ in 0..10 {
        let delay = backoff.delay();
        assert!(
            delay <= Duration::from_secs(1),
            "{delay:?} exceeded the cap"
        );
    }

    // A run that lasted clears the history.
    backoff.reset();
    assert_eq!(backoff.attempts(), 0);
    assert!(backoff.delay() < Duration::from_millis(200));
}

#[test]
fn a_status_change_is_published_as_a_state_topic() {
    let hub = Hub::new();
    let units = UnitTable::detached(hub.clone());
    let name = unit("battery-widget");

    units.transition(&name, Transition::Spawned);

    // Supervision is observable through the state plane, like everything else
    // the daemon knows.
    let patch = hub.read_state(&[SystemTopic::Units.as_str().to_string()]);
    let topic = &patch.topics[0];
    assert_eq!(topic.topic, "units");

    match topic.value.as_ref().unwrap() {
        omega_proto::omega::state_topic::Value::Units(units) => {
            assert_eq!(units.units.len(), 1);
            assert_eq!(units.units[0].unit, "battery-widget");
            // Spawned is not yet running: the unit has not checked in.
            assert_eq!(units.units[0].phase, UnitPhase::Starting as i32);
        }
        other => panic!("expected the units topic, got {other:?}"),
    }
}

#[test]
fn a_repeated_status_is_not_a_new_revision() {
    let hub = Hub::new();
    let units = UnitTable::detached(hub.clone());
    let name = unit("battery-widget");

    units.transition(&name, Transition::Spawned);
    units.transition(&name, Transition::Spawned);

    let patch = hub.read_state(&[SystemTopic::Units.as_str().to_string()]);
    let revision = patch.topics[0].revision;

    units.transition(
        &name,
        Transition::Exited {
            code: 1,
            detail: "exit status: 1".into(),
        },
    );
    let patch = hub.read_state(&[SystemTopic::Units.as_str().to_string()]);
    assert!(patch.topics[0].revision > revision, "a real change is news");
}

/// Whether this patch is the table reporting a unit as failed — one arrives
/// per spawn the supervisor could not make.
fn is_failure(patch: &omega_proto::omega::StatePatch) -> bool {
    patch.topics.iter().any(|topic| {
        matches!(
            topic.value.as_ref(),
            Some(omega_proto::omega::state_topic::Value::Units(units))
                if units.units.iter().any(|s| s.phase == UnitPhase::Failed as i32)
        )
    })
}

#[tokio::test(start_paused = true)]
async fn a_unit_that_cannot_be_spawned_is_reported_failed_and_paced() {
    let shutdown = Shutdown::new();
    let hub = Hub::new();
    let units = UnitTable::detached(hub.clone());
    units.adopt(&ManifestStore::from_manifests([widget_manifest(
        "missing-unit",
        "battery",
    )]));
    let supervisor = Supervisor::new(
        Socket::at("/tmp/omega-supervision-test.sock"),
        units,
        shutdown.clone(),
    );

    let (_, mut patches) = hub.subscribe_state();

    // A binary that does not exist: the supervisor must report it rather than
    // spin silently.
    supervisor.spawn(UnitSpec::new(unit("missing-unit"), "/nonexistent/omega"));

    // Nothing here sleeps for real: the clock jumps to each backoff in turn,
    // so what the assertion below measures is the pacing itself.
    let started = tokio::time::Instant::now();
    let mut attempts = 0;
    while attempts < 3 {
        match patches.recv().await {
            Ok(patch) if is_failure(&patch) => attempts += 1,
            Ok(_) => {}
            // Restarts arriving faster than the state plane can carry them
            // is itself the pacing failing.
            Err(tokio::sync::broadcast::error::RecvError::Lagged(missed)) => {
                panic!("{missed} restarts were reported faster than the backoff allows")
            }
            Err(e) => panic!("the hub stopped reporting: {e}"),
        }
    }
    let waited = started.elapsed();

    let statuses = supervisor.statuses();
    assert_eq!(statuses[0].unit, "missing-unit");
    // Not merely non-empty: a bare "No such file or directory" is what this
    // reads as to whoever runs `omega status`, and the missing path is the
    // only part of it that says anything.
    assert!(
        statuses[0].detail.contains("/nonexistent/omega"),
        "a unit that cannot be spawned names the program it could not run, \
         got {:?}",
        statuses[0].detail
    );

    // Three attempts means two waits: the base delay, then twice it. Jitter
    // moves each by a fraction, never by a factor.
    let paced = (Backoff::BASE + Backoff::BASE * 2).mul_f64(0.75);
    assert!(
        waited >= paced,
        "{waited:?} is faster than the backoff allows ({paced:?})"
    );
    assert!(
        waited < Backoff::BASE * 8,
        "{waited:?} is slower than the backoff asks for"
    );

    // And it stops when the daemon does, rather than respawning forever.
    shutdown.trigger();
    tokio::time::timeout(Duration::from_secs(30), async {
        while !supervisor.all_stopped() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("shutdown must end the restart loop");
}

#[tokio::test]
async fn a_token_is_revoked_when_its_process_is_gone() {
    let supervisor = Supervisor::new(
        Socket::at("/tmp/omega-supervision-token.sock"),
        table_with(ManifestStore::default()),
        Shutdown::new(),
    );

    let token: UnitToken = supervisor.register(&unit("battery-widget"));
    assert!(supervisor.identify(1234, token.as_str()).is_some());
    assert!(supervisor.identify(1234, "another-token").is_none());
}

#[tokio::test(start_paused = true)]
async fn cancelling_a_supervisor_releases_its_unit_and_token() {
    let units = UnitTable::detached(Hub::new());
    let shutdown = Shutdown::new();
    let supervisor = Supervisor::new(Socket::at("/unused"), units.clone(), shutdown.clone());
    let name = unit("cancelled");
    let task = supervisor.spawn(UnitSpec::new(name.clone(), "/nonexistent/omega"));
    tokio::task::yield_now().await;
    assert!(units.is_supervised(&name));
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    assert!(!units.is_supervised(&name));
    assert!(!shutdown.is_triggered());
}
