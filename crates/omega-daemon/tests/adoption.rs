//! Taking a unit's place: what `omega dev` asks the daemon for.

mod common;

use std::time::Duration;

use common::{Harness, expect_outcome, expect_refusal, next_result, unit_name, widget_manifest};
use omega_daemon::manifest::ManifestStore;
use omega_daemon::units::UnitRecord;
use omega_proto::omega::{
    AdoptUnit, ErrorCode, Frame, Invoke, UnitPhase, Welcome, frame, invoke, result, value,
};
use omega_proto::{Refusal, Transport};

fn adopt(stream_id: u64, unit: &str) -> Frame {
    Frame {
        stream_id,
        body: Some(frame::Body::Invoke(Invoke {
            op: Some(invoke::Op::AdoptUnit(AdoptUnit { unit: unit.into() })),
        })),
    }
}

fn harness(tag: &str) -> Harness {
    Harness::new(
        tag,
        ManifestStore::from_manifests([widget_manifest("battery-widget", "battery")]),
    )
}

/// Ask as the operator, and take the token back.
async fn adopted(transport: &mut Transport<tokio::net::UnixStream>, unit: &str) -> String {
    transport.send(adopt(1, unit)).await.unwrap();
    match expect_outcome(next_result(transport).await) {
        result::Outcome::Value(value) => match value.kind {
            Some(value::Kind::StringValue(token)) => token,
            other => panic!("AdoptUnit answered with {other:?}"),
        },
        other => panic!("AdoptUnit answered with {other:?}"),
    }
}

fn welcome(frame: Option<Frame>) -> Welcome {
    match frame.expect("expected a Welcome, got EOF").body {
        Some(frame::Body::Welcome(welcome)) => welcome,
        other => panic!("expected a Welcome, got {other:?}"),
    }
}

/// Poll until the asynchronous adoption cleanup completes.
async fn until(within: Duration, done: impl Fn() -> bool) {
    let deadline = tokio::time::Instant::now() + within;
    while tokio::time::Instant::now() < deadline {
        if done() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("the daemon did not get there within {within:?}");
}

#[tokio::test]
async fn a_token_the_daemon_hands_out_makes_this_process_the_unit() {
    let harness = harness("adopt-identity");
    let name = unit_name("battery-widget");

    let mut operator = harness.connect("operator", "").await;
    operator.recv().await.unwrap().unwrap(); // Welcome

    let token = adopted(&mut operator, "battery-widget").await;

    // The token is what a spawned unit would have been given, and it admits
    // this process as that unit — with the manifest's grants, not more.
    let hash = widget_manifest("battery-widget", "battery").hash();
    let mut unit = harness.connect(&hash, &token).await;
    let welcome = welcome(unit.recv().await.unwrap());

    assert_eq!(welcome.unit_id, name.to_string());
    assert!(
        !welcome.capabilities.is_empty(),
        "an adopted unit is granted what its manifest declares"
    );

    // Adopted lifecycle reports the replacement process.
    until(Duration::from_secs(2), || {
        phase_of(&harness, &name) == UnitPhase::Running as i32
    })
    .await;
}

fn phase_of(harness: &Harness, name: &omega_proto::UnitName) -> i32 {
    harness
        .units
        .statuses()
        .into_iter()
        .find(|status| status.unit == name.to_string())
        .map(|status| status.phase)
        .unwrap_or_default()
}

#[tokio::test]
async fn an_adopted_unit_is_not_started_underneath_the_process_developing_it() {
    let harness = harness("adopt-held");
    let name = unit_name("battery-widget");

    let mut operator = harness.connect("operator", "").await;
    operator.recv().await.unwrap().unwrap();
    adopted(&mut operator, "battery-widget").await;

    // Active adoption prevents convergence from spawning another process.
    assert!(
        harness.supervisor.running().contains(&name),
        "an adopted unit must count as held, or the built binary races the one being written"
    );

    // Status must identify the adopted process.
    let status = harness
        .units
        .statuses()
        .into_iter()
        .find(|status| status.unit == name.to_string())
        .expect("the unit is in the table");
    assert_eq!(status.detail, UnitRecord::ADOPTED);
}

#[tokio::test]
async fn an_adoption_ends_with_the_connection_that_asked_for_it() {
    let harness = harness("adopt-release");
    let name = unit_name("battery-widget");

    let mut operator = harness.connect("operator", "").await;
    operator.recv().await.unwrap().unwrap();
    let token = adopted(&mut operator, "battery-widget").await;

    // Closing the terminal is how a dev session usually ends, and the unit
    // has to come back from it.
    drop(operator);
    until(Duration::from_secs(2), || {
        !harness.supervisor.running().contains(&name)
    })
    .await;

    // An expired adoption token grants no unit authority.
    let hash = widget_manifest("battery-widget", "battery").hash();
    let mut late = harness.connect(&hash, &token).await;
    let welcome = welcome(late.recv().await.unwrap());

    assert_ne!(welcome.unit_id, name.to_string());
    assert!(welcome.unit_id.starts_with("operator-"), "{welcome:?}");
    assert!(welcome.capabilities.is_empty(), "{welcome:?}");
}

#[tokio::test]
async fn a_unit_may_not_take_another_units_place() {
    let harness = harness("adopt-denied");
    let token = harness.register_unit("battery-widget");
    let hash = widget_manifest("battery-widget", "battery").hash();

    let mut unit = harness.connect(&hash, token.as_str()).await;
    unit.recv().await.unwrap().unwrap(); // Welcome

    unit.send(adopt(1, "battery-widget")).await.unwrap();
    let refusal: Refusal = expect_refusal(next_result(&mut unit).await);

    // Handing out identity is the owner's business. A unit that could adopt
    // its neighbour could become it.
    assert_eq!(refusal.code, ErrorCode::PermissionDenied);
}

#[tokio::test]
async fn adopting_something_this_build_never_produced_is_refused() {
    let harness = harness("adopt-unknown");

    let mut operator = harness.connect("operator", "").await;
    operator.recv().await.unwrap().unwrap();

    operator.send(adopt(1, "clock")).await.unwrap();
    let refusal = expect_refusal(next_result(&mut operator).await);

    // A token for a name with no manifest is a token the handshake would
    // refuse a moment later; refusing it here says why.
    assert_eq!(refusal.code, ErrorCode::FailedPrecondition);
    assert!(refusal.message.contains("clock"), "{refusal}");
}
