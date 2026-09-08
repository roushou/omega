//! Turning NetworkManager's answers into the ontology.
//!
//! NetworkManager's vocabulary and the ontology's disagree about almost
//! everything: a state ladder against a boolean, a connection-type string
//! against an enum, and two different names for the network. The walk that
//! gathers the answers needs a bus; deciding what they mean does not.

use std::time::Duration;

use omega_brokers::network_manager::Reading;
use omega_brokers::{Broker, NetworkManager};
use omega_proto::omega::{NetworkType, state_topic};

/// Associated with a Wi-Fi network, with internet.
fn wifi() -> Reading {
    Reading {
        manager_state: 70,
        kind: "802-11-wireless".into(),
        id: "home".into(),
        interface: "wlp2s0".into(),
        ssid: "Sommiee_5G".into(),
        strength: 47,
        is_vpn: false,
    }
}

#[test]
fn a_state_ladder_becomes_a_boolean() {
    assert!(wifi().state().connected);

    // Connecting is not connected, and neither is disconnecting: a widget
    // that read "not zero" as "on a network" would flicker on every attempt.
    for state in [0, 10, 20, 30, 40] {
        let reading = Reading {
            manager_state: state,
            ..wifi()
        };
        assert!(
            !reading.state().connected,
            "NMState {state} is not connected"
        );
    }

    // Connected to a network that does not reach the internet is still
    // connected to a network. Whether it reaches the internet is a different
    // question and not the one this field asks.
    for state in [50, 60, 70] {
        assert!(
            Reading {
                manager_state: state,
                ..wifi()
            }
            .state()
            .connected
        );
    }
}

#[test]
fn a_connection_type_becomes_a_kind() {
    let kind = |nm: &str| {
        Reading {
            kind: nm.into(),
            ..wifi()
        }
        .state()
        .r#type
    };

    assert_eq!(kind("802-11-wireless"), NetworkType::Wifi as i32);
    assert_eq!(kind("802-3-ethernet"), NetworkType::Ethernet as i32);
    assert_eq!(kind("gsm"), NetworkType::Cellular as i32);
    // A type this build has no arm for is unspecified rather than guessed at.
    assert_eq!(kind("bluetooth"), NetworkType::Unspecified as i32);
}

#[test]
fn a_vpn_is_reported_as_one_however_it_is_spelled() {
    // NetworkManager says it twice: a flag on the active connection, and a
    // type string. A reading that trusted only the string would call a
    // WireGuard tunnel Wi-Fi, because the flag is what the manager sets.
    let flagged = Reading {
        is_vpn: true,
        ..wifi()
    };
    assert_eq!(flagged.state().r#type, NetworkType::Vpn as i32);

    let typed = Reading {
        kind: "wireguard".into(),
        is_vpn: false,
        ..wifi()
    };
    assert_eq!(typed.state().r#type, NetworkType::Vpn as i32);
}

#[test]
fn the_access_point_names_the_network_and_the_connection_is_the_fallback() {
    // A connection can be renamed; an SSID cannot. So the access point wins.
    assert_eq!(wifi().state().ssid, "Sommiee_5G");

    // But a blank label while the association settles looks broken, so the
    // connection's own name stands in until the access point is there.
    let associating = Reading {
        ssid: String::new(),
        ..wifi()
    };
    assert_eq!(associating.state().ssid, "home");
}

#[test]
fn a_wired_connection_has_no_signal_and_says_which_it_is() {
    let wired = Reading {
        kind: "802-3-ethernet".into(),
        interface: "enp0s31f6".into(),
        ssid: String::new(),
        id: "Wired connection 1".into(),
        strength: 0,
        ..wifi()
    };
    let state = wired.state();

    assert!(state.connected);
    assert_eq!(state.signal_percent, 0, "a cable has no bars");
    assert_eq!(state.r#type, NetworkType::Ethernet as i32);
    assert_eq!(state.interface, "enp0s31f6");
}

// ---- against the machine this is running on ----

#[tokio::test]
#[ignore = "needs a system bus with NetworkManager; run with --ignored"]
async fn it_reads_the_machine_it_is_running_on() {
    let mut network = NetworkManager::new();

    let patch = network.next().await.expect("NetworkManager answered");
    assert_eq!(patch.topics.len(), 1);
    assert_eq!(patch.topics[0].topic, "network");

    let Some(state_topic::Value::Network(state)) = patch.topics[0].value.as_ref() else {
        panic!("expected a network reading");
    };
    // Disconnected is a reading too, so the only claim that always holds is
    // that a connected machine could say what it is connected through.
    if state.connected {
        assert_ne!(
            state.interface, "",
            "a connected machine is connected through something"
        );
    }

    // The second reading waits — for a signal, or for the refresh that puts a
    // floor under them. It must not come back instantly, or the broker is a
    // hot loop wearing a signal stream.
    let again = tokio::time::timeout(Duration::from_millis(500), network.next()).await;
    assert!(again.is_err(), "a second reading should wait");
}
