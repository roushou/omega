//! What an admitted unit may ask for. Every refusal is answered, never
//! silently dropped.

mod common;

use std::time::Duration;

use common::{Harness, expect_refusal, unit_name, widget_manifest};
use omega_daemon::hub::SurfaceRef;
use omega_daemon::manifest::ManifestStore;
use omega_proto::omega::{
    CallCommand, ErrorCode, Frame, Invoke, PublishView, ViewNode, ViewTree, frame, invoke,
};

fn surface(id: &str) -> omega_proto::SurfaceId {
    omega_proto::SurfaceId::parse(id).unwrap()
}

fn view() -> ViewTree {
    ViewTree {
        root: Some(ViewNode {
            key: "battery".into(),
            r#type: "box".into(),
            props: Default::default(),
            children: Vec::new(),
            ..Default::default()
        }),
        revision: 0,
    }
}

fn publish(surface: &str) -> Frame {
    Frame {
        stream_id: 7,
        body: Some(frame::Body::Invoke(Invoke {
            op: Some(invoke::Op::PublishView(PublishView {
                surface_id: surface.into(),
                module_id: String::new(),
                view: Some(view()),
            })),
        })),
    }
}

/// A harness with one unit that declares the `battery` widget surface.
async fn harness(tag: &str) -> (Harness, String, omega_daemon::units::UnitToken) {
    let manifest = widget_manifest("battery-widget", "battery");
    let harness = Harness::new(tag, ManifestStore::from_manifests([manifest.clone()]));
    let token = harness.register_unit("battery-widget");
    (harness, manifest.hash(), token)
}

#[tokio::test]
async fn a_unit_may_publish_to_a_surface_it_declared() {
    let (harness, hash, token) = harness("publish-own").await;
    let (_, mut views) = harness.hub.subscribe_views();

    let mut transport = harness.connect(&hash, token.as_str()).await;
    transport.recv().await.unwrap().unwrap(); // Welcome
    transport.send(publish("battery")).await.unwrap();

    let update = tokio::time::timeout(Duration::from_secs(2), views.recv())
        .await
        .unwrap()
        .unwrap();

    // The surface is qualified by the unit the daemon authenticated, not by
    // anything the frame said.
    assert_eq!(
        update.surface,
        SurfaceRef::new(unit_name("battery-widget"), surface("battery"))
    );
}

#[tokio::test]
async fn publishing_to_an_undeclared_surface_is_refused() {
    let (harness, hash, token) = harness("publish-other").await;
    let (_, mut views) = harness.hub.subscribe_views();

    let mut transport = harness.connect(&hash, token.as_str()).await;
    transport.recv().await.unwrap().unwrap(); // Welcome

    // "clock" belongs to some other unit; this one never declared it.
    transport.send(publish("clock")).await.unwrap();

    let refusal = expect_refusal(transport.recv().await.unwrap());
    assert_eq!(refusal.code, ErrorCode::PermissionDenied);
    assert!(refusal.message.contains("clock"), "{refusal}");
    assert!(
        views.try_recv().is_err(),
        "a refused publish must not reach subscribers"
    );
    assert!(harness.hub.view_snapshot().is_empty());
}

#[tokio::test]
async fn a_refusal_answers_the_stream_it_refused() {
    let (harness, hash, token) = harness("refusal-stream").await;

    let mut transport = harness.connect(&hash, token.as_str()).await;
    transport.recv().await.unwrap().unwrap(); // Welcome
    transport.send(publish("clock")).await.unwrap();

    let frame = transport.recv().await.unwrap().unwrap();
    assert_eq!(frame.stream_id, 7, "a refusal correlates with its request");
}

#[tokio::test]
async fn an_op_this_daemon_does_not_serve_is_refused_not_ignored() {
    let (harness, hash, token) = harness("unimplemented").await;

    let mut transport = harness.connect(&hash, token.as_str()).await;
    transport.recv().await.unwrap().unwrap(); // Welcome

    // `CallCommand` is the daemon asking a unit to run one of its own
    // commands — it travels the other way down this socket and has no policy
    // row for a unit to reach. A unit sending one is asking to invoke its
    // neighbours, and the answer is an answer rather than silence.
    transport
        .send(Frame {
            stream_id: 1,
            body: Some(frame::Body::Invoke(Invoke {
                op: Some(invoke::Op::CallCommand(CallCommand {
                    command: "connect".into(),
                    args: Vec::new(),
                })),
            })),
        })
        .await
        .unwrap();

    let refusal = expect_refusal(transport.recv().await.unwrap());
    assert_eq!(refusal.code, ErrorCode::Unimplemented);
    assert!(refusal.message.contains("CallCommand"), "{refusal}");
}

#[tokio::test]
async fn an_operator_cannot_publish_as_a_unit() {
    let manifest = widget_manifest("battery-widget", "battery");
    let harness = Harness::new(
        "operator-publish",
        ManifestStore::from_manifests([manifest]),
    );

    let mut transport = harness.connect("whatever", "").await;
    transport.recv().await.unwrap().unwrap(); // Welcome

    transport.send(publish("battery")).await.unwrap();

    let refusal = expect_refusal(transport.recv().await.unwrap());
    assert_eq!(refusal.code, ErrorCode::PermissionDenied);
    assert!(harness.hub.view_snapshot().is_empty());
}
