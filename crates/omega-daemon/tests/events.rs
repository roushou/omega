//! Events: derived from state, delivered by declaration, emitted by plugins.

mod common;

use std::time::Duration;

use common::{Harness, expect_refusal, next_result, policy_manifest, widget_manifest};
use omega_daemon::events::Transitions;
use omega_daemon::manifest::ManifestStore;
use omega_proto::Manifest;
use omega_proto::omega::{
    BatteryState, CustomEvent, EmitEvent, ErrorCode, EventKind, Frame, Invoke, StatePatch,
    StateTopic, Subscribe, event, frame, invoke, state_topic,
};

fn battery(level: f64, charging: bool) -> StatePatch {
    StatePatch {
        topics: vec![StateTopic {
            topic: "battery".into(),
            revision: 0,
            value: Some(state_topic::Value::Battery(BatteryState {
                level,
                charging,
                seconds_to_empty: 0,
                seconds_to_full: 0,
            })),
        }],
    }
}

async fn connected(
    tag: &str,
    manifest: Manifest,
) -> (Harness, omega_proto::Transport<tokio::net::UnixStream>) {
    let harness = Harness::new(tag, ManifestStore::from_manifests([manifest.clone()]));
    let token = harness.register_plugin(manifest.name.as_str());
    let mut transport = harness.connect(&manifest.hash(), token.as_str()).await;
    transport.recv().await.unwrap().unwrap(); // Welcome
    (harness, transport)
}

/// The next event frame, skipping the state patches that caused it.
async fn next_event(
    transport: &mut omega_proto::Transport<tokio::net::UnixStream>,
) -> omega_proto::omega::Event {
    loop {
        let frame = tokio::time::timeout(Duration::from_secs(2), transport.recv())
            .await
            .expect("timed out waiting for an event")
            .unwrap()
            .unwrap();
        if let Some(frame::Body::Event(event)) = frame.body {
            return event;
        }
    }
}

fn mains(connected: bool) -> StatePatch {
    StatePatch {
        topics: vec![StateTopic {
            topic: "mains".into(),
            revision: 0,
            value: Some(state_topic::Value::Mains(omega_proto::omega::MainsState {
                connected,
            })),
        }],
    }
}

fn idle(idle: bool) -> StatePatch {
    StatePatch {
        topics: vec![StateTopic {
            topic: "idle".into(),
            revision: 0,
            value: Some(state_topic::Value::Idle(omega_proto::omega::IdleState {
                idle,
                idle_since: 0,
                locked: false,
            })),
        }],
    }
}

fn clipboard(text: &str) -> StatePatch {
    StatePatch {
        topics: vec![StateTopic {
            topic: "clipboard".into(),
            revision: 0,
            value: Some(state_topic::Value::Clipboard(
                omega_proto::omega::ClipboardState { text: text.into() },
            )),
        }],
    }
}

fn kinds(events: Vec<(EventKind, Option<event::Detail>)>) -> Vec<EventKind> {
    events.into_iter().map(|(kind, _)| kind).collect()
}

#[test]
fn cable_events_follow_mains_not_battery_charging() {
    let mut transitions = Transitions::new();
    assert!(transitions.of(&mains(true)).is_empty());
    assert!(transitions.of(&battery(0.9, true)).is_empty());
    assert!(transitions.of(&battery(1.0, false)).is_empty());
    assert_eq!(
        kinds(transitions.of(&mains(false))),
        vec![EventKind::EventAcUnplugged]
    );
    assert_eq!(
        kinds(transitions.of(&mains(true))),
        vec![EventKind::EventAcPlugged]
    );
}

#[test]
fn a_battery_threshold_fires_on_the_way_down_and_only_once() {
    let mut transitions = Transitions::new();
    transitions.of(&battery(0.5, false));

    assert_eq!(
        kinds(transitions.of(&battery(0.14, false))),
        vec![EventKind::EventBatteryLow]
    );
    // Still low is not newly low.
    assert!(transitions.of(&battery(0.13, false)).is_empty());

    assert_eq!(
        kinds(transitions.of(&battery(0.04, false))),
        vec![EventKind::EventBatteryCritical]
    );

    // Charging back up past the threshold does not fire it again...
    assert!(transitions.of(&battery(0.5, true)).is_empty());
    // ...but the next fall does.
    assert_eq!(
        kinds(transitions.of(&battery(0.1, true))),
        vec![EventKind::EventBatteryLow]
    );
}

#[test]
fn a_battery_crossing_carries_the_level_that_crossed() {
    let mut transitions = Transitions::new();
    transitions.of(&battery(0.5, false));
    let events = transitions.of(&battery(0.14, false));
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].0, EventKind::EventBatteryLow);
    assert_eq!(
        events[0].1,
        Some(event::Detail::Power(omega_proto::omega::PowerEvent {
            battery_percent: 0.14,
        }))
    );
}

#[test]
fn idle_transitions_fire_once_per_crossing() {
    let mut transitions = Transitions::new();
    assert!(transitions.of(&idle(true)).is_empty());
    assert_eq!(
        kinds(transitions.of(&idle(false))),
        vec![EventKind::EventIdleExited]
    );
    assert!(transitions.of(&idle(false)).is_empty());
    assert_eq!(
        kinds(transitions.of(&idle(true))),
        vec![EventKind::EventIdleEntered]
    );
}

#[test]
fn a_clipboard_change_fires_once_per_content() {
    let mut transitions = Transitions::new();
    assert!(transitions.of(&clipboard("first")).is_empty());
    assert_eq!(
        kinds(transitions.of(&clipboard("second"))),
        vec![EventKind::EventClipboardChanged]
    );
    // The same content is not a new change.
    assert!(transitions.of(&clipboard("second")).is_empty());
}

#[tokio::test]
async fn a_plugin_receives_the_events_its_manifest_declares() {
    let (harness, mut transport) = connected("events-declared", policy_manifest("policy")).await;

    harness.hub.publish_state(mains(false)).unwrap();
    harness.hub.publish_state(mains(true)).unwrap();

    let event = next_event(&mut transport).await;
    assert_eq!(event.kind, EventKind::EventAcPlugged as i32);
    assert!(event.id > 0, "the daemon stamps event identity");
    assert!(event.timestamp_ns > 0);
    assert!(matches!(event.detail, Some(event::Detail::Power(_))));
}

#[tokio::test]
async fn a_plugin_is_not_woken_for_events_it_never_declared() {
    // This manifest declares no events at all.
    let (harness, mut transport) =
        connected("events-undeclared", widget_manifest("reader", "battery")).await;

    harness.hub.publish_state(mains(false)).unwrap();
    harness.hub.publish_state(mains(true)).unwrap();

    // The state patches arrive; the event does not.
    let deadline = tokio::time::Instant::now() + Duration::from_millis(400);
    while tokio::time::Instant::now() < deadline {
        let Ok(Ok(Some(frame))) =
            tokio::time::timeout(Duration::from_millis(100), transport.recv()).await
        else {
            continue;
        };
        assert!(
            !matches!(frame.body, Some(frame::Body::Event(_))),
            "an undeclared event must not reach the plugin"
        );
    }
}

#[tokio::test]
async fn subscribing_to_an_undeclared_event_is_refused() {
    let (_harness, mut transport) = connected("events-widen", policy_manifest("policy")).await;

    transport
        .send(Frame {
            stream_id: 1,
            body: Some(frame::Body::Invoke(Invoke {
                op: Some(invoke::Op::Subscribe(Subscribe {
                    topics: Vec::new(),
                    events: vec![EventKind::EventWindowFocused as i32],
                    replace: false,
                })),
            })),
        })
        .await
        .unwrap();

    let refusal = expect_refusal(next_result(&mut transport).await);
    assert_eq!(refusal.code, ErrorCode::PermissionDenied);
    assert!(
        refusal.message.contains("EVENT_WINDOW_FOCUSED"),
        "{refusal}"
    );
}

#[tokio::test]
async fn an_emitted_event_carries_the_identity_the_daemon_authenticated() {
    let (_harness, mut transport) = connected("events-emit", policy_manifest("policy")).await;

    transport
        .send(Frame {
            stream_id: 1,
            body: Some(frame::Body::Invoke(Invoke {
                op: Some(invoke::Op::EmitEvent(EmitEvent {
                    event: Some(CustomEvent {
                        // A lie: this plugin is "policy".
                        plugin: "some-other-plugin".into(),
                        name: "threshold-crossed".into(),
                        payload: None,
                    }),
                })),
            })),
        })
        .await
        .unwrap();

    // The plugin declares EVENT_CUSTOM, so it receives its own event back.
    let event = next_event(&mut transport).await;
    match event.detail {
        Some(event::Detail::Custom(custom)) => {
            assert_eq!(custom.name, "threshold-crossed");
            assert_eq!(custom.plugin, "policy", "the daemon names the emitter");
        }
        other => panic!("expected a custom event, got {other:?}"),
    }
}

#[tokio::test]
async fn a_plugin_receives_the_idle_and_clipboard_events_it_declares() {
    let manifest = widget_manifest("idle", "battery").handling([
        EventKind::EventIdleEntered,
        EventKind::EventIdleExited,
        EventKind::EventClipboardChanged,
    ]);
    let (harness, mut transport) = connected("events-idle-clipboard", manifest).await;

    harness.hub.publish_state(idle(true)).unwrap(); // first reading, no event
    harness.hub.publish_state(idle(false)).unwrap();
    let exited = next_event(&mut transport).await;
    assert_eq!(exited.kind, EventKind::EventIdleExited as i32);
    assert!(matches!(exited.detail, Some(event::Detail::Idle(_))));
    harness.hub.publish_state(idle(true)).unwrap();
    let entered = next_event(&mut transport).await;
    assert_eq!(entered.kind, EventKind::EventIdleEntered as i32);

    harness.hub.publish_state(clipboard("before")).unwrap();
    harness.hub.publish_state(clipboard("after")).unwrap();
    let changed = next_event(&mut transport).await;
    assert_eq!(changed.kind, EventKind::EventClipboardChanged as i32);
    assert!(matches!(changed.detail, Some(event::Detail::Clipboard(_))));
}
