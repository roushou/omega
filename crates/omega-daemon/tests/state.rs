//! State: last-value-wins, reads bounded by the manifest, and a unit's own
//! keyspace.

mod common;

use std::time::Duration;

use common::{
    Harness, expect_ok, expect_outcome, expect_refusal, next_result, widget_manifest,
    writer_manifest,
};
use omega_daemon::manifest::ManifestStore;
use omega_daemon::state::StateStore;
use omega_proto::Manifest;
use omega_proto::omega::{
    AudioState, BatteryState, ErrorCode, Frame, GetState, Invoke, SetState, StatePatch, StateTopic,
    Subscribe, Unsubscribe, Value, frame, invoke, result, state_topic, value,
};

fn battery(level: f64) -> StatePatch {
    StatePatch {
        topics: vec![StateTopic {
            topic: "battery".into(),
            revision: 0,
            value: Some(state_topic::Value::Battery(BatteryState {
                level,
                charging: false,
                seconds_to_empty: 0,
                seconds_to_full: 0,
            })),
        }],
    }
}

fn op(stream_id: u64, op: invoke::Op) -> Frame {
    Frame {
        stream_id,
        body: Some(frame::Body::Invoke(Invoke { op: Some(op) })),
    }
}

async fn connected(
    tag: &str,
    manifest: Manifest,
) -> (Harness, omega_proto::Transport<tokio::net::UnixStream>) {
    let harness = Harness::new(tag, ManifestStore::from_manifests([manifest.clone()]));
    let token = harness.register_unit(manifest.name.as_str());
    let mut transport = harness.connect(&manifest.hash(), token.as_str()).await;
    transport.recv().await.unwrap().unwrap(); // Welcome
    (harness, transport)
}

#[test]
fn an_unchanged_value_is_not_an_update() {
    let mut store = StateStore::new();

    let first = store.apply(battery(0.5)).unwrap();
    assert_eq!(first.topics[0].revision, 1);

    // A source polling every two seconds republishes the same reading; that
    // is not news, and waking every unit for it would be pure churn.
    assert!(store.apply(battery(0.5)).unwrap().topics.is_empty());

    let changed = store.apply(battery(0.6)).unwrap();
    assert_eq!(changed.topics[0].revision, 2, "revisions count changes");
}

#[tokio::test]
async fn the_welcome_mirror_carries_only_declared_topics() {
    let manifest = widget_manifest("reader", "battery");
    let harness = Harness::new(
        "welcome-scope",
        ManifestStore::from_manifests([manifest.clone()]),
    );

    // Two topics exist; the unit declared one of them.
    harness.hub.publish_state(battery(0.5)).unwrap();
    harness
        .hub
        .publish_state(StatePatch {
            topics: vec![StateTopic {
                topic: "audio".into(),
                revision: 0,
                value: Some(state_topic::Value::Audio(AudioState {
                    volume: 0.3,
                    muted: false,
                    default_sink: "sink".into(),
                })),
            }],
        })
        .unwrap();

    let token = harness.register_unit("reader");
    let mut transport = harness.connect(&manifest.hash(), token.as_str()).await;

    match transport.recv().await.unwrap().unwrap().body {
        Some(frame::Body::Welcome(welcome)) => {
            let topics: Vec<_> = welcome
                .state
                .unwrap()
                .topics
                .into_iter()
                .map(|t| t.topic)
                .collect();
            assert_eq!(topics, vec!["battery".to_string()]);
        }
        other => panic!("expected Welcome, got {other:?}"),
    }
}

#[tokio::test]
async fn get_state_answers_on_the_stream_that_asked() {
    let (harness, mut transport) =
        connected("get-state", widget_manifest("reader", "battery")).await;
    harness.hub.publish_state(battery(0.5)).unwrap();

    transport
        .send(op(
            9,
            invoke::Op::GetState(GetState {
                topics: vec!["battery".into()],
            }),
        ))
        .await
        .unwrap();

    let frame = next_result(&mut transport).await.unwrap();
    assert_eq!(frame.stream_id, 9);
    match expect_outcome(Some(frame)) {
        result::Outcome::State(patch) => assert_eq!(patch.topics[0].topic, "battery"),
        other => panic!("expected state, got {other:?}"),
    }
}

#[tokio::test]
async fn reading_a_topic_the_manifest_never_declared_is_refused() {
    let (_harness, mut transport) =
        connected("get-undeclared", widget_manifest("reader", "battery")).await;

    transport
        .send(op(
            1,
            invoke::Op::GetState(GetState {
                topics: vec!["audio".into()],
            }),
        ))
        .await
        .unwrap();

    let refusal = expect_refusal(next_result(&mut transport).await);
    assert_eq!(refusal.code, ErrorCode::PermissionDenied);
    assert!(refusal.message.contains("audio"), "{refusal}");
}

#[tokio::test]
async fn a_unit_is_woken_only_for_what_it_subscribes_to() {
    let (harness, mut transport) =
        connected("subscribe", widget_manifest("reader", "battery")).await;

    // Unsubscribed: the patch is stored but this unit is not woken for it.
    transport
        .send(op(
            1,
            invoke::Op::Unsubscribe(Unsubscribe {
                topics: vec!["battery".into()],
                events: Vec::new(),
            }),
        ))
        .await
        .unwrap();
    expect_ok(next_result(&mut transport).await);

    harness.hub.publish_state(battery(0.5)).unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(200), transport.recv())
            .await
            .is_err(),
        "an unsubscribed topic must not reach the unit"
    );

    // Subscribed again: the next change arrives.
    transport
        .send(op(
            2,
            invoke::Op::Subscribe(Subscribe {
                topics: vec!["battery".into()],
                events: Vec::new(),
                replace: false,
            }),
        ))
        .await
        .unwrap();
    expect_ok(next_result(&mut transport).await);

    harness.hub.publish_state(battery(0.9)).unwrap();
    let frame = tokio::time::timeout(Duration::from_secs(2), transport.recv())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(matches!(frame.body, Some(frame::Body::StatePatch(_))));
}

#[tokio::test]
async fn subscribing_is_not_a_way_to_widen_a_grant() {
    let (_harness, mut transport) =
        connected("subscribe-wide", widget_manifest("reader", "battery")).await;

    transport
        .send(op(
            1,
            invoke::Op::Subscribe(Subscribe {
                topics: vec!["network".into()],
                events: Vec::new(),
                replace: false,
            }),
        ))
        .await
        .unwrap();

    let refusal = expect_refusal(next_result(&mut transport).await);
    assert_eq!(refusal.code, ErrorCode::PermissionDenied);
}

#[tokio::test]
async fn writing_state_without_the_capability_is_denied() {
    // The manifest grants STATE_READ only.
    let (_harness, mut transport) =
        connected("write-denied", widget_manifest("reader", "battery")).await;

    transport
        .send(op(
            1,
            invoke::Op::SetState(SetState {
                topic: "unit.reader.threshold".into(),
                value: Some(Value {
                    kind: Some(value::Kind::IntValue(20)),
                }),
            }),
        ))
        .await
        .unwrap();

    let refusal = expect_refusal(next_result(&mut transport).await);
    assert_eq!(refusal.code, ErrorCode::PermissionDenied);
    assert!(
        refusal.message.contains("CAPABILITY_STATE_WRITE"),
        "{refusal}"
    );
}

#[tokio::test]
async fn a_unit_writes_its_own_keyspace_and_nothing_else() {
    let (harness, mut transport) = connected("write-own", writer_manifest("writer")).await;

    transport
        .send(op(
            1,
            invoke::Op::SetState(SetState {
                topic: "unit.writer.threshold".into(),
                value: Some(Value {
                    kind: Some(value::Kind::IntValue(20)),
                }),
            }),
        ))
        .await
        .unwrap();
    expect_ok(next_result(&mut transport).await);

    // The daemon owns it now, and replicates it back like any other topic.
    let stored = harness
        .hub
        .read_state(&["unit.writer.threshold".to_string()]);
    assert_eq!(stored.topics[0].revision, 1);

    // Another unit's keyspace, and the daemon's own topics, stay closed.
    for topic in ["unit.reader.threshold", "battery"] {
        transport
            .send(op(
                2,
                invoke::Op::SetState(SetState {
                    topic: topic.into(),
                    value: Some(Value {
                        kind: Some(value::Kind::IntValue(1)),
                    }),
                }),
            ))
            .await
            .unwrap();
        let refusal = expect_refusal(next_result(&mut transport).await);
        assert_eq!(refusal.code, ErrorCode::PermissionDenied, "{topic}");
    }
}

#[tokio::test]
async fn state_budget_refusals_are_correlated_and_leave_the_connection_usable() {
    let (harness, transport) = connected("state-budget", writer_manifest("writer")).await;
    let (mut reader, mut writer) = transport.split();
    let (results, mut answers) = tokio::sync::mpsc::channel(8);
    // Large state echoes must be drained while the other socket half writes.
    let receiving = tokio::spawn(async move {
        while let Some(frame) = reader.recv().await.unwrap() {
            if matches!(&frame.body, Some(frame::Body::Result(_))) {
                results.send(frame).await.unwrap();
            }
        }
    });
    for (stream, topic, size, expected) in [
        (1, "unit.writer.first", 600_000, None),
        (
            3,
            "unit.writer.second",
            600_000,
            Some(ErrorCode::ResourceExhausted),
        ),
        (
            5,
            "unit.writer.second",
            1_100_000,
            Some(ErrorCode::PayloadTooLarge),
        ),
        (7, "unit.writer.second", 1, None),
    ] {
        writer
            .send(op(
                stream,
                invoke::Op::SetState(SetState {
                    topic: topic.into(),
                    value: Some(Value {
                        kind: Some(value::Kind::StringValue("x".repeat(size))),
                    }),
                }),
            ))
            .await
            .unwrap();
        let frame = tokio::time::timeout(Duration::from_secs(2), answers.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(frame.stream_id, stream);
        if let Some(expected) = expected {
            assert_eq!(expect_refusal(Some(frame)).code, expected);
            assert!(
                harness
                    .hub
                    .read_state(&["unit.writer.second".into()])
                    .topics
                    .is_empty()
            );
        } else {
            expect_ok(Some(frame));
        }
    }
    assert_eq!(
        harness
            .hub
            .read_state(&["unit.writer.second".into()])
            .topics[0]
            .revision,
        1
    );
    receiving.abort();
    let _ = receiving.await;
}

#[tokio::test]
async fn client_and_daemon_keep_receiving_during_repeated_large_writes() {
    let manifest = writer_manifest("writer");
    let harness = Harness::new(
        "duplex-state",
        ManifestStore::from_manifests([manifest.clone()]),
    );
    let token = harness.register_unit("writer");
    let (peer, server) = tokio::net::UnixStream::pair().unwrap();
    let shutdown = omega_daemon::Shutdown::new();
    let session = omega_daemon::Session::new(harness.supervisor.clone(), harness.hub.clone())
        .with_units(harness.units.clone())
        .with_shutdown(shutdown.clone());
    let serving = tokio::spawn(session.serve(server));
    let (mut client, _) = omega_proto::Client::over(peer, &manifest.hash(), token.as_str())
        .await
        .unwrap();
    for version in 0..6 {
        let stream = client.allocate();
        client
            .invoke(
                stream,
                invoke::Op::SetState(SetState {
                    topic: "unit.writer.large".into(),
                    value: Some(Value {
                        kind: Some(value::Kind::StringValue(format!(
                            "{version}{}",
                            "x".repeat(600_000)
                        ))),
                    }),
                }),
            )
            .await
            .unwrap();
        assert!(matches!(
            client.answer(stream).await.unwrap(),
            result::Outcome::Ok(_)
        ));
    }
    assert_eq!(
        harness.hub.read_state(&["unit.writer.large".into()]).topics[0].revision,
        6
    );
    shutdown.trigger();
    serving.await.unwrap().unwrap();
    drop(client);
}
