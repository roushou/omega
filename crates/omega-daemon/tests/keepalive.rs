//! Keepalive: the daemon checks its peers, and answers when checked.

mod common;

use std::time::Duration;

use common::{Harness, widget_manifest};
use omega_daemon::manifest::ManifestStore;
use omega_daemon::session::{Health, Liveness};
use omega_wire::omega::{Frame, Ping, frame};

#[tokio::test(start_paused = true)]
async fn a_peer_is_declared_unresponsive_only_after_the_timeout() {
    // The cadence a desktop actually runs, asserted on a clock the test moves
    // rather than waited out.
    let mut liveness = Liveness::new();

    assert_eq!(liveness.health(), Health::Alive);

    tokio::time::advance(Liveness::TIMEOUT - Duration::from_secs(1)).await;
    assert_eq!(
        liveness.health(),
        Health::Alive,
        "a quiet peer inside the timeout is still alive"
    );

    tokio::time::advance(Duration::from_secs(2)).await;
    assert_eq!(liveness.health(), Health::Unresponsive);

    // Any frame from the peer is proof of life, not just a Pong.
    liveness.seen();
    assert_eq!(liveness.health(), Health::Alive);
}

/// The whole point of the keepalive: a peer that never answers does not hold
/// its session open forever.
#[tokio::test(start_paused = true)]
async fn a_peer_that_stops_answering_is_closed() {
    let manifest = widget_manifest("battery-widget", "battery");
    let harness = Harness::new("silent", ManifestStore::from_manifests([manifest.clone()]));
    let token = harness.register_unit("battery-widget");

    let mut transport = harness.connect(&manifest.hash(), token.as_str()).await;
    transport.recv().await.unwrap().unwrap(); // Welcome

    // Pings arrive and go unanswered; nothing here moves the clock, so the
    // only thing that can end this loop is the daemon giving up.
    let closed = tokio::time::timeout(Duration::from_secs(600), async {
        while transport.recv().await.unwrap().is_some() {}
    })
    .await;

    assert!(
        closed.is_ok(),
        "a peer that never answers must not hold its session open"
    );
}

#[tokio::test]
async fn the_daemon_answers_a_peer_s_ping() {
    let manifest = widget_manifest("battery-widget", "battery");
    let harness = Harness::new("ping", ManifestStore::from_manifests([manifest.clone()]));
    let token = harness.register_unit("battery-widget");

    let mut transport = harness.connect(&manifest.hash(), token.as_str()).await;
    transport.recv().await.unwrap().unwrap(); // Welcome

    transport
        .send(Frame {
            stream_id: 3,
            body: Some(frame::Body::Ping(Ping { nonce: 42 })),
        })
        .await
        .unwrap();

    let pong = transport.recv().await.unwrap().unwrap();
    assert_eq!(pong.stream_id, 3);
    match pong.body {
        Some(frame::Body::Pong(pong)) => assert_eq!(pong.nonce, 42),
        other => panic!("expected Pong, got {other:?}"),
    }
}
