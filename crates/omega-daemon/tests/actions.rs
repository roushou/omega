//! Actions: what a unit may make the machine do, and what it may not.

mod common;

use std::time::Duration;

use common::{Harness, expect_ok, expect_refusal, next_result, policy_manifest, widget_manifest};
use omega_daemon::action::ActionKind;
use omega_daemon::manifest::ManifestStore;
use omega_manifest::Manifest;
use omega_wire::omega::{
    Act, Action, Capability, ErrorCode, Frame, Invoke, Lock, RunCommand, action, frame, invoke,
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

async fn connected(
    tag: &str,
    manifest: Manifest,
) -> (Harness, omega_wire::Transport<tokio::net::UnixStream>) {
    let harness = Harness::new(tag, ManifestStore::from_manifests([manifest.clone()]));
    let token = harness.register_unit(manifest.name.as_str());
    let mut transport = harness.connect(&manifest.hash(), token.as_str()).await;
    transport.recv().await.unwrap().unwrap(); // Welcome
    (harness, transport)
}

#[test]
fn every_action_states_what_it_costs() {
    // The escape hatch and the power button are not the same permission.
    assert_eq!(ActionKind::RunCommand.cost(), Some(Capability::Spawn));
    assert_eq!(ActionKind::Shutdown.cost(), Some(Capability::SystemControl));
    assert_eq!(ActionKind::SetBacklight.cost(), Some(Capability::Backlight));
    assert_eq!(ActionKind::Notify.cost(), Some(Capability::Notify));

    // Actions that only move a unit's own windows around cost nothing extra.
    assert_eq!(ActionKind::ToggleFullscreen.cost(), None);
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
    let manifest = Manifest {
        capabilities: vec![
            "CAPABILITY_STATE_READ".into(),
            "CAPABILITY_SYSTEM_CONTROL".into(),
        ],
        ..widget_manifest("locker", "battery")
    };
    let (_harness, mut transport) = connected("act-unimplemented", manifest).await;

    transport
        .send(act(1, action::Kind::Lock(Lock {})))
        .await
        .unwrap();

    let refusal = expect_refusal(next_result(&mut transport).await);
    assert_eq!(refusal.code, ErrorCode::Unimplemented);
    assert!(refusal.message.contains("Lock"), "{refusal}");
}
