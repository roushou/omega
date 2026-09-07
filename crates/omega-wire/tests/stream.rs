//! The two ends of a connection cannot pick the same stream id.

use omega_wire::{DaemonStreams, PeerStreams};

#[test]
fn the_two_ends_never_collide() {
    let mut daemon = DaemonStreams::new();
    let mut peer = PeerStreams::new();

    let daemon_ids: Vec<u64> = (0..8).map(|_| daemon.allocate()).collect();
    let peer_ids: Vec<u64> = (0..8).map(|_| peer.allocate()).collect();

    assert_eq!(daemon_ids, vec![2, 4, 6, 8, 10, 12, 14, 16]);
    assert_eq!(peer_ids, vec![1, 3, 5, 7, 9, 11, 13, 15]);
    assert!(
        daemon_ids.iter().all(|id| !peer_ids.contains(id)),
        "an answer has to be unambiguous"
    );
}

#[test]
fn each_end_recognises_its_own_answers() {
    let mut daemon = DaemonStreams::new();
    let mut peer = PeerStreams::new();

    let asked_by_daemon = daemon.allocate();
    let asked_by_peer = peer.allocate();

    assert!(DaemonStreams::is_ours(asked_by_daemon));
    assert!(!DaemonStreams::is_ours(asked_by_peer));

    assert!(PeerStreams::is_ours(asked_by_peer));
    assert!(!PeerStreams::is_ours(asked_by_daemon));

    // Stream 0 is fire-and-forget: nobody allocated it, so nobody is waiting
    // on an answer to it.
    assert!(!DaemonStreams::is_ours(0));
}
