//! Lifecycle: what the daemon's owner may ask of it, and what a unit may not.

mod common;

use std::time::Duration;

use common::{Harness, expect_ok, expect_refusal, next_result, widget_manifest};
use omega_core::UnitName;
use omega_daemon::hub::Hub;
use omega_daemon::manifest::ManifestStore;
use omega_daemon::supervisor::{UnitLog, UnitSpec};
use omega_wire::omega::{ErrorCode, Frame, Invoke, RestartUnit, frame, invoke};

fn restart(stream_id: u64, unit: &str) -> Frame {
    Frame {
        stream_id,
        body: Some(frame::Body::Invoke(Invoke {
            op: Some(invoke::Op::RestartUnit(RestartUnit { unit: unit.into() })),
        })),
    }
}

fn unit(name: &str) -> UnitName {
    UnitName::parse(name).unwrap()
}

#[tokio::test]
async fn an_operator_can_cycle_a_running_unit() {
    let harness = Harness::new(
        "operator-restart",
        ManifestStore::from_manifests([widget_manifest("sleeper", "battery")]),
    );

    // A unit that stays up until something stops it.
    let script = std::env::temp_dir().join(format!("omega-sleeper-{}.sh", std::process::id()));
    std::fs::write(&script, "#!/bin/sh\nsleep 30\n").unwrap();
    std::fs::set_permissions(
        &script,
        <std::fs::Permissions as std::os::unix::fs::PermissionsExt>::from_mode(0o755),
    )
    .unwrap();
    harness
        .supervisor
        .spawn(UnitSpec::new(unit("sleeper"), &script));

    let started = tokio::time::Instant::now() + Duration::from_secs(2);
    while tokio::time::Instant::now() < started && harness.supervisor.running().is_empty() {
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    // The operator presents no token; the uid is the whole claim.
    let mut transport = harness.connect("operator", "").await;
    transport.recv().await.unwrap().unwrap(); // Welcome

    transport.send(restart(1, "sleeper")).await.unwrap();
    expect_ok(next_result(&mut transport).await);

    // Cycled, not stopped: the unit is still supervised afterwards.
    assert!(harness.supervisor.running().contains(&unit("sleeper")));

    let _ = std::fs::remove_file(&script);
}

#[tokio::test]
async fn restarting_something_that_is_not_running_says_so() {
    let harness = Harness::new("operator-unknown", ManifestStore::default());

    let mut transport = harness.connect("operator", "").await;
    transport.recv().await.unwrap().unwrap(); // Welcome

    transport.send(restart(1, "not-a-unit")).await.unwrap();

    let refusal = expect_refusal(next_result(&mut transport).await);
    assert_eq!(refusal.code, ErrorCode::InvalidArgument);
    assert!(refusal.message.contains("not running"), "{refusal}");
}

#[tokio::test]
async fn a_unit_may_not_restart_its_neighbours() {
    let manifest = widget_manifest("battery-widget", "battery");
    let harness = Harness::new(
        "unit-restart",
        ManifestStore::from_manifests([manifest.clone()]),
    );
    let token = harness.register_unit("battery-widget");

    let mut transport = harness.connect(&manifest.hash(), token.as_str()).await;
    transport.recv().await.unwrap().unwrap(); // Welcome

    // Lifecycle belongs to whoever owns the daemon, not to what it runs.
    transport.send(restart(1, "battery-widget")).await.unwrap();

    let refusal = expect_refusal(next_result(&mut transport).await);
    assert_eq!(refusal.code, ErrorCode::PermissionDenied);
    assert!(refusal.message.contains("not served to units"), "{refusal}");
}

#[tokio::test]
async fn an_operator_may_not_read_a_units_state() {
    let harness = Harness::new(
        "operator-getstate",
        ManifestStore::from_manifests([widget_manifest("reader", "battery")]),
    );
    let _ = Hub::new();
    let _ = UnitLog::at("/dev/null");

    let mut transport = harness.connect("operator", "").await;
    transport.recv().await.unwrap().unwrap(); // Welcome

    transport
        .send(Frame {
            stream_id: 1,
            body: Some(frame::Body::Invoke(Invoke {
                op: Some(invoke::Op::GetState(omega_wire::omega::GetState {
                    topics: vec!["battery".into()],
                })),
            })),
        })
        .await
        .unwrap();

    // State reads are a unit's business, gated by a manifest an operator does
    // not have. What the operator can watch, it watches on the read-only
    // observation socket.
    let refusal = expect_refusal(next_result(&mut transport).await);
    assert_eq!(refusal.code, ErrorCode::PermissionDenied);
}
