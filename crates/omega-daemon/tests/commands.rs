//! Command surfaces: the daemon asking a unit to do something.

mod common;

use std::time::Duration;

use common::{Harness, command_manifest, expect_refusal, next_result, widget_manifest};
use omega_daemon::manifest::ManifestStore;
use omega_proto::Manifest;
use omega_proto::omega::{
    Act, Action, ErrorCode, Frame, Invoke, InvokeUnit, Value, action, frame, invoke, result, value,
};

fn call(stream_id: u64, unit: &str, command: &str) -> Frame {
    Frame {
        stream_id,
        body: Some(frame::Body::Invoke(Invoke {
            op: Some(invoke::Op::Act(Act {
                action: Some(Action {
                    kind: Some(action::Kind::InvokeUnit(InvokeUnit {
                        unit: unit.into(),
                        command: command.into(),
                        args: vec![Value {
                            kind: Some(value::Kind::StringValue("now".into())),
                        }],
                    })),
                }),
            })),
        })),
    }
}

async fn connected_unit(
    harness: &Harness,
    manifest: &Manifest,
) -> omega_proto::Transport<tokio::net::UnixStream> {
    let token = harness.register_unit(manifest.name.as_str());
    let mut transport = harness.connect(&manifest.hash(), token.as_str()).await;
    transport.recv().await.unwrap().unwrap(); // Welcome
    transport
}

#[tokio::test]
async fn an_operator_calls_a_command_and_gets_its_answer() {
    let manifest = command_manifest("lamp", "toggle");
    let harness = Harness::new(
        "command-call",
        ManifestStore::from_manifests([manifest.clone()]),
    );

    // The unit that serves the command...
    let mut unit = connected_unit(&harness, &manifest).await;

    // ...and the operator asking for it.
    let mut operator = harness.connect("operator", "").await;
    operator.recv().await.unwrap().unwrap(); // Welcome
    operator.send(call(1, "lamp", "toggle")).await.unwrap();

    // The unit sees the call on one of the daemon's own streams, with the
    // arguments it was given.
    let frame = tokio::time::timeout(Duration::from_secs(2), unit.recv())
        .await
        .expect("the daemon should forward the call")
        .unwrap()
        .unwrap();
    assert!(frame.stream_id % 2 == 0, "daemon streams are even");

    let Some(frame::Body::Invoke(Invoke {
        op: Some(invoke::Op::CallCommand(called)),
    })) = frame.body
    else {
        panic!("expected a CallCommand");
    };
    assert_eq!(called.command, "toggle");
    // Not just the count: a button binds its arguments to say which row was
    // pressed, and an argument replaced by a default would be a list of forty
    // networks that all connect to the same one.
    assert_eq!(
        called.args,
        vec![Value {
            kind: Some(value::Kind::StringValue("now".into())),
        }]
    );

    // The unit answers, and the answer reaches the operator.
    unit.send(Frame {
        stream_id: frame.stream_id,
        body: Some(frame::Body::Result(omega_proto::omega::Result {
            outcome: Some(result::Outcome::Value(Value {
                kind: Some(value::Kind::StringValue("on".into())),
            })),
            done: true,
        })),
    })
    .await
    .unwrap();

    let answer = next_result(&mut operator).await.unwrap();
    assert_eq!(answer.stream_id, 1, "answered on the stream that asked");
    match common::expect_outcome(Some(answer)) {
        result::Outcome::Value(value) => {
            assert_eq!(value.kind, Some(value::Kind::StringValue("on".into())))
        }
        other => panic!("expected the unit's answer, got {other:?}"),
    }
}

#[tokio::test]
async fn a_command_the_unit_never_declared_is_refused_before_it_is_asked() {
    let manifest = command_manifest("lamp", "toggle");
    let harness = Harness::new(
        "command-undeclared",
        ManifestStore::from_manifests([manifest.clone()]),
    );
    let _unit = connected_unit(&harness, &manifest).await;

    let mut operator = harness.connect("operator", "").await;
    operator.recv().await.unwrap().unwrap(); // Welcome
    operator.send(call(1, "lamp", "togle")).await.unwrap();

    // The daemon knows the manifest, so a typo is answered here — with what
    // the unit does declare — rather than by the unit inventing an error.
    let refusal = expect_refusal(next_result(&mut operator).await);
    assert_eq!(refusal.code, ErrorCode::InvalidArgument);
    assert!(refusal.message.contains("toggle"), "{refusal}");
}

#[tokio::test]
async fn a_unit_that_declares_no_commands_says_so() {
    let manifest = widget_manifest("battery-widget", "battery");
    let harness = Harness::new(
        "command-none",
        ManifestStore::from_manifests([manifest.clone()]),
    );
    let _unit = connected_unit(&harness, &manifest).await;

    let mut operator = harness.connect("operator", "").await;
    operator.recv().await.unwrap().unwrap(); // Welcome
    operator
        .send(call(1, "battery-widget", "toggle"))
        .await
        .unwrap();

    let refusal = expect_refusal(next_result(&mut operator).await);
    assert!(refusal.message.contains("no command surfaces"), "{refusal}");
}

#[tokio::test]
async fn calling_a_unit_that_is_not_connected_is_refused() {
    let manifest = command_manifest("lamp", "toggle");
    let harness = Harness::new("command-absent", ManifestStore::from_manifests([manifest]));

    let mut operator = harness.connect("operator", "").await;
    operator.recv().await.unwrap().unwrap(); // Welcome
    operator.send(call(1, "lamp", "toggle")).await.unwrap();

    let refusal = expect_refusal(next_result(&mut operator).await);
    // Not a bad request: the caller asked for something reasonable of a unit
    // that is not there yet.
    assert_eq!(refusal.code, ErrorCode::FailedPrecondition);
    assert!(refusal.message.contains("not connected"), "{refusal}");
}

#[tokio::test]
async fn a_units_own_refusal_reaches_the_caller_as_it_was() {
    let manifest = command_manifest("lamp", "toggle");
    let harness = Harness::new(
        "command-refused",
        ManifestStore::from_manifests([manifest.clone()]),
    );
    let mut unit = connected_unit(&harness, &manifest).await;

    let mut operator = harness.connect("operator", "").await;
    operator.recv().await.unwrap().unwrap(); // Welcome
    operator.send(call(1, "lamp", "toggle")).await.unwrap();

    let frame = tokio::time::timeout(Duration::from_secs(2), unit.recv())
        .await
        .expect("the daemon should forward the call")
        .unwrap()
        .unwrap();

    // The unit says no, in its own words and with its own code.
    unit.send(
        omega_proto::Refusal::denied("the lamp is bolted to the wall").frame(frame.stream_id),
    )
    .await
    .unwrap();

    // The daemon was the messenger; flattening the unit's answer into one of
    // its own would lose why.
    let refusal = expect_refusal(next_result(&mut operator).await);
    assert_eq!(refusal.code, ErrorCode::PermissionDenied);
    assert!(refusal.message.contains("bolted to the wall"), "{refusal}");
}

#[tokio::test]
async fn a_unit_needs_a_capability_to_invoke_another() {
    let lamp = command_manifest("lamp", "toggle");
    let caller = widget_manifest("battery-widget", "battery");
    let harness = Harness::new(
        "command-unit-caller",
        ManifestStore::from_manifests([lamp.clone(), caller.clone()]),
    );
    let _lamp = connected_unit(&harness, &lamp).await;
    let mut caller_unit = connected_unit(&harness, &caller).await;

    // Making another unit run its own code is making code run, and this
    // manifest was never granted that.
    caller_unit.send(call(3, "lamp", "toggle")).await.unwrap();

    let refusal = expect_refusal(next_result(&mut caller_unit).await);
    assert_eq!(refusal.code, ErrorCode::PermissionDenied);
    assert!(refusal.message.contains("CAPABILITY_SPAWN"), "{refusal}");
}
