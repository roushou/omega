//! Admission: who gets a session, and what the daemon tells the rest.

mod common;

use std::time::Duration;

use common::{Harness, widget_manifest};
use omega_daemon::manifest::ManifestStore;
use omega_proto::omega::{BatteryState, Capability, StatePatch, StateTopic, frame, state_topic};

fn store() -> ManifestStore {
    ManifestStore::from_manifests([widget_manifest("test-unit", "battery")])
}

#[tokio::test]
async fn a_spawned_unit_is_admitted_and_mirrors_state() {
    let harness = Harness::new("handshake", store());
    let token = harness.register_unit("test-unit");
    let hash = widget_manifest("test-unit", "battery").hash();

    let mut transport = harness.connect(&hash, token.as_str()).await;

    let welcome = transport.recv().await.unwrap().unwrap();
    match welcome.body {
        Some(frame::Body::Welcome(w)) => {
            assert_eq!(w.protocol_version, omega_proto::PROTOCOL_VERSION);
            assert_eq!(w.unit_id, "test-unit");
            // Grants come from the daemon's manifest, never from the peer.
            assert_eq!(w.capabilities, vec![Capability::StateRead as i32]);
        }
        other => panic!("expected Welcome, got {other:?}"),
    }

    harness
        .hub
        .publish_state(StatePatch {
            topics: vec![StateTopic {
                topic: "battery".into(),
                revision: 0,
                value: Some(state_topic::Value::Battery(BatteryState {
                    level: 0.75,
                    charging: true,
                    seconds_to_empty: 0,
                    seconds_to_full: 0,
                })),
            }],
        })
        .unwrap();

    let patch = tokio::time::timeout(Duration::from_secs(2), transport.recv())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    match patch.body {
        Some(frame::Body::StatePatch(p)) => {
            assert_eq!(p.topics[0].topic, "battery");
            // The Hub assigns the revision; sources don't.
            assert_eq!(p.topics[0].revision, 1);
        }
        other => panic!("expected StatePatch, got {other:?}"),
    }
}

#[tokio::test]
async fn a_token_belonging_to_another_process_is_refused() {
    let harness = Harness::new("wrong-pid", store());
    let token = harness.register_unit("test-unit");
    let hash = widget_manifest("test-unit", "battery").hash();

    // The first connection binds the token to this process...
    let mut first = harness.connect(&hash, token.as_str()).await;
    assert!(matches!(
        first.recv().await.unwrap().unwrap().body,
        Some(frame::Body::Welcome(_))
    ));

    // ...and the registry now rejects the same token from any other pid.
    assert!(
        harness
            .supervisor
            .identify(std::process::id() as i32, token.as_str())
            .is_some()
    );
    assert!(
        harness
            .supervisor
            .identify(std::process::id() as i32 + 1, token.as_str())
            .is_none()
    );
}

#[tokio::test]
async fn a_peer_with_no_token_is_the_operator() {
    let harness = Harness::new("operator", store());
    harness.register_unit("test-unit");

    // Not a unit — but the daemon's own user, who can already signal it and
    // rewrite its state dir. It is admitted as what it is.
    let mut transport = harness.connect("no-manifest-of-my-own", "").await;

    match transport.recv().await.unwrap().unwrap().body {
        Some(frame::Body::Welcome(w)) => {
            assert!(w.unit_id.starts_with("operator-"), "{}", w.unit_id);
            // Owning the daemon is not the same as being a unit: an operator
            // holds no capabilities and no surfaces.
            assert!(w.capabilities.is_empty());
        }
        other => panic!("expected Welcome, got {other:?}"),
    }
}
