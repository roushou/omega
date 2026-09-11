//! Actions: what a unit may make the machine do, and what it may not.

mod common;

use std::time::Duration;

use common::{Harness, expect_ok, expect_refusal, next_result, policy_manifest, widget_manifest};
use omega_daemon::manifest::ManifestStore;
use omega_proto::Manifest;
use std::sync::{Arc, Mutex};

use omega_proto::ActionKind;
use omega_proto::omega::{
    Act, Action, Capability, ErrorCode, Frame, Invoke, Lock, RunCommand, SetBacklight, StatePatch,
    action, frame, invoke, set_backlight,
};

fn act(stream_id: u64, kind: action::Kind) -> Frame {
    Frame {
        stream_id,
        body: Some(frame::Body::Invoke(Invoke {
            op: Some(invoke::Op::Act(Act {
                action: Some(Action { kind: Some(kind) }),
            })),
        })),
    }
}

/// A broker that serves one kind and remembers being asked.
///
/// Standing in for the real one on purpose: what the daemon owes is that an
/// action reaches the broker claiming its kind. Whether sysfs then took the
/// write is `omega-brokers`' business, and dragging a device tree in here
/// would test that twice and this once.
#[derive(Debug, Clone, Default)]
struct Recorder {
    served: Arc<Mutex<Vec<ActionKind>>>,
}

#[async_trait::async_trait]
impl omega_brokers::Broker for Recorder {
    fn name(&self) -> &'static str {
        "recorder"
    }

    // Reports nothing, ever: a broker may exist only to be asked, and the
    // trait's defaults are what say so.
    fn topics(&self) -> &'static [omega_proto::SystemTopic] {
        &[]
    }

    fn actions(&self) -> &'static [ActionKind] {
        &[ActionKind::SetBacklight]
    }

    async fn act(
        &mut self,
        action: &action::Kind,
    ) -> Result<Option<StatePatch>, omega_brokers::BrokerError> {
        self.served
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(ActionKind::of(action));
        Ok(None)
    }
}

fn dim() -> action::Kind {
    action::Kind::SetBacklight(SetBacklight {
        change: Some(set_backlight::Change::AbsolutePercent(40)),
    })
}

fn backlight_manifest(name: &str) -> Manifest {
    widget_manifest(name, "battery").granting([Capability::StateRead, Capability::Backlight])
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

#[tokio::test]
async fn a_unit_without_the_capability_cannot_run_a_command() {
    // This manifest grants STATE_READ only.
    let (_harness, mut transport) =
        connected("act-denied", widget_manifest("reader", "battery")).await;

    transport
        .send(act(
            1,
            action::Kind::RunCommand(RunCommand {
                command: "touch /tmp/omega-should-not-exist".into(),
            }),
        ))
        .await
        .unwrap();

    let refusal = expect_refusal(next_result(&mut transport).await);
    assert_eq!(refusal.code, ErrorCode::PermissionDenied);
    assert!(refusal.message.contains("CAPABILITY_SPAWN"), "{refusal}");
    assert!(!std::path::Path::new("/tmp/omega-should-not-exist").exists());
}

#[tokio::test]
async fn a_granted_command_actually_runs() {
    let (_harness, mut transport) = connected("act-run", policy_manifest("policy")).await;

    let marker = std::env::temp_dir().join(format!("omega-act-{}", std::process::id()));
    let _ = std::fs::remove_file(&marker);

    transport
        .send(act(
            1,
            action::Kind::RunCommand(RunCommand {
                command: format!("touch {}", marker.display()),
            }),
        ))
        .await
        .unwrap();

    expect_ok(next_result(&mut transport).await);

    // The daemon answers without waiting for the command, so the effect
    // lands a moment later.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    while tokio::time::Instant::now() < deadline && !marker.exists() {
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(marker.exists(), "the command should have run");
    let _ = std::fs::remove_file(&marker);
}

#[tokio::test]
async fn an_action_is_refused_for_its_capability_before_its_implementation() {
    // `policy` may spawn, but it may not control the machine — and Lock is
    // not implemented either. The capability is the reason it hears about,
    // so a missing handler can never be mistaken for a grant.
    let (_harness, mut transport) = connected("act-order", policy_manifest("policy")).await;

    transport
        .send(act(1, action::Kind::Lock(Lock {})))
        .await
        .unwrap();

    let refusal = expect_refusal(next_result(&mut transport).await);
    assert_eq!(refusal.code, ErrorCode::PermissionDenied);
    assert!(
        refusal.message.contains("CAPABILITY_SYSTEM_CONTROL"),
        "{refusal}"
    );
}

#[tokio::test]
async fn an_authorized_but_unperformable_action_says_so() {
    let manifest = widget_manifest("locker", "battery")
        .granting([Capability::StateRead, Capability::SystemControl]);
    let (_harness, mut transport) = connected("act-unimplemented", manifest).await;

    transport
        .send(act(1, action::Kind::Lock(Lock {})))
        .await
        .unwrap();

    let refusal = expect_refusal(next_result(&mut transport).await);
    assert_eq!(refusal.code, ErrorCode::Unimplemented);
    assert!(refusal.message.contains("Lock"), "{refusal}");
}

#[tokio::test]
async fn an_action_reaches_the_broker_that_claims_its_kind() {
    let recorder = Recorder::default();
    let manifest = backlight_manifest("dimmer");
    let harness = Harness::new(
        "act-brokered",
        ManifestStore::from_manifests([manifest.clone()]),
    )
    .with_broker(Box::new(recorder.clone()));

    let token = harness.register_unit(manifest.name.as_str());
    let mut transport = harness.connect(&manifest.hash(), token.as_str()).await;
    transport.recv().await.unwrap().unwrap(); // Welcome

    transport.send(act(1, dim())).await.unwrap();
    expect_ok(next_result(&mut transport).await);

    assert_eq!(
        *recorder.served.lock().unwrap(),
        vec![ActionKind::SetBacklight],
        "the broker that claimed the kind is the one that was asked"
    );
}

#[tokio::test]
async fn an_action_no_broker_claims_is_still_unimplemented() {
    // The same request, on a daemon running no broker for it. Authorization
    // already passed — so the answer has to be that nothing can do it, not
    // silence, or a granted capability would stand in for a handler that does
    // not exist.
    let (_harness, mut transport) = connected("act-unbrokered", backlight_manifest("dimmer")).await;

    transport.send(act(1, dim())).await.unwrap();

    let refusal = expect_refusal(next_result(&mut transport).await);
    assert_eq!(refusal.code, ErrorCode::Unimplemented);
    assert!(refusal.message.contains("SetBacklight"), "{refusal}");
}

#[tokio::test]
async fn invalid_payloads_are_refused_before_broker_dispatch() {
    let recorder = Recorder::default();
    let manifest = backlight_manifest("dimmer");
    let harness = Harness::new(
        "act-malformed",
        ManifestStore::from_manifests([manifest.clone()]),
    )
    .with_broker(Box::new(recorder.clone()));
    let token = harness.register_unit(&manifest.name);
    let mut transport = harness.connect(&manifest.hash(), token.as_str()).await;
    transport.recv().await.unwrap().unwrap();
    for (stream, change) in [
        (1, None),
        (3, Some(set_backlight::Change::AbsolutePercent(101))),
    ] {
        transport
            .send(act(
                stream,
                action::Kind::SetBacklight(SetBacklight { change }),
            ))
            .await
            .unwrap();
        let answer = next_result(&mut transport).await;
        assert_eq!(answer.as_ref().unwrap().stream_id, stream);
        let refusal = expect_refusal(answer);
        assert_eq!(refusal.code, ErrorCode::InvalidArgument);
        assert!(refusal.message.contains("SetBacklight.change"));
    }
    assert!(recorder.served.lock().unwrap().is_empty());
    transport.send(act(5, dim())).await.unwrap();
    expect_ok(next_result(&mut transport).await);
    assert_eq!(
        *recorder.served.lock().unwrap(),
        vec![ActionKind::SetBacklight]
    );
}

#[tokio::test]
async fn capability_refusal_precedes_payload_validation() {
    let (_harness, mut transport) =
        connected("act-malformed-denied", widget_manifest("reader", "battery")).await;
    transport
        .send(act(1, action::Kind::SetBacklight(SetBacklight::default())))
        .await
        .unwrap();
    let refusal = expect_refusal(next_result(&mut transport).await);
    assert_eq!(refusal.code, ErrorCode::PermissionDenied);
}

#[tokio::test]
async fn nonfinite_binary_volume_changes_are_invalid_even_without_a_handler() {
    let manifest =
        widget_manifest("volume", "panel").granting([Capability::StateRead, Capability::Audio]);
    let (_harness, mut transport) = connected("act-nonfinite", manifest).await;
    for (index, value) in [f64::NAN, f64::INFINITY, -0.1, 1.1].into_iter().enumerate() {
        let stream = (index * 2 + 1) as u64;
        transport
            .send(act(
                stream,
                action::Kind::SetVolume(omega_proto::omega::SetVolume {
                    change: Some(omega_proto::omega::set_volume::Change::Absolute(value)),
                }),
            ))
            .await
            .unwrap();
        let answer = next_result(&mut transport).await;
        assert_eq!(answer.as_ref().unwrap().stream_id, stream);
        let refusal = expect_refusal(answer);
        assert_eq!(refusal.code, ErrorCode::InvalidArgument);
        assert!(refusal.message.contains("SetVolume.change.absolute"));
    }
}
