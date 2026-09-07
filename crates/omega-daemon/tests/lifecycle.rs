//! A unit's life, as a state machine rather than a series of reports.

use omega_daemon::hub::Hub;
use omega_daemon::units::{Lifecycle, Transition, UnitTable};
use omega_proto::UnitName;
use omega_proto::omega::UnitPhase;

fn unit(name: &str) -> UnitName {
    UnitName::parse(name).unwrap()
}

fn table() -> UnitTable {
    UnitTable::detached(Hub::new())
}

fn phase(units: &UnitTable, name: &UnitName) -> i32 {
    units
        .statuses()
        .into_iter()
        .find(|status| status.unit == name.as_str())
        .expect("the unit should be in the table")
        .phase
}

#[test]
fn a_spawned_unit_is_starting_until_it_checks_in() {
    let units = table();
    let name = unit("battery-widget");
    let token = units.issue(&name);

    units.transition(&name, Transition::Spawned);

    // A process exists, but the daemon has not vouched for it: it has not
    // presented a manifest hash yet, and saying "running" would claim more
    // than the daemon knows.
    assert_eq!(phase(&units, &name), UnitPhase::Starting as i32);

    // The handshake is what makes it running.
    let (requests, _answers) = tokio::sync::mpsc::channel(1);
    assert_eq!(units.identify(4242, token.as_str()), Some(name.clone()));
    let _session = units.connected(&name, requests);

    assert_eq!(phase(&units, &name), UnitPhase::Running as i32);
}

#[test]
fn a_unit_that_loses_its_session_is_no_longer_running() {
    let units = table();
    let name = unit("battery-widget");
    units.transition(&name, Transition::Spawned);

    let (requests, _answers) = tokio::sync::mpsc::channel(1);
    let session = units.connected(&name, requests);
    assert_eq!(phase(&units, &name), UnitPhase::Running as i32);

    // The process may still be up, but a unit the daemon cannot reach is not
    // one it should report as running.
    drop(session);
    assert_eq!(phase(&units, &name), UnitPhase::Starting as i32);
    assert!(!units.is_connected(&name));
}

#[test]
fn an_exit_carries_its_reason_into_the_status() {
    let units = table();
    let name = unit("battery-widget");

    units.transition(&name, Transition::Spawned);
    units.transition(
        &name,
        Transition::Exited {
            code: 101,
            detail: "exit status: 101".into(),
        },
    );

    let status = units
        .statuses()
        .into_iter()
        .find(|status| status.unit == name.as_str())
        .unwrap();
    assert_eq!(status.phase, UnitPhase::Restarting as i32);
    assert_eq!(status.last_exit_code, 101);
    assert_eq!(status.detail, "exit status: 101");
}

#[test]
fn restarts_count_spawns_after_the_first() {
    let units = table();
    let name = unit("battery-widget");

    units.transition(&name, Transition::Spawned);
    assert_eq!(units.statuses()[0].restarts, 0);

    for expected in 1..=3 {
        units.transition(
            &name,
            Transition::Exited {
                code: 1,
                detail: "exit status: 1".into(),
            },
        );
        units.transition(&name, Transition::Spawned);
        assert_eq!(units.statuses()[0].restarts, expected);
    }
}

#[test]
fn a_stopped_unit_stays_stopped_until_something_spawns_it() {
    let mut lifecycle = Lifecycle::Running;

    assert!(lifecycle.apply(Transition::Stopped));
    assert_eq!(lifecycle, Lifecycle::Stopped);

    // A late exit report from the process that was just killed must not
    // resurrect it as "restarting".
    assert!(!lifecycle.apply(Transition::Exited {
        code: 0,
        detail: String::new(),
    }));
    assert_eq!(lifecycle, Lifecycle::Stopped);

    // Only a new spawn does.
    assert!(lifecycle.apply(Transition::Spawned));
    assert_eq!(lifecycle, Lifecycle::Starting);
}

#[test]
fn a_peer_the_supervisor_never_spawned_has_no_lifecycle_to_change() {
    let mut lifecycle = Lifecycle::Idle;

    // An operator's session, or a unit registered by hand in a test: being
    // connected does not mean the supervisor is running it.
    assert!(!lifecycle.connected());
    assert_eq!(lifecycle, Lifecycle::Idle);
}
