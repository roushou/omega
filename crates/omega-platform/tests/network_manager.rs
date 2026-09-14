//! NetworkManager state, connection type, and access-point conversion tests.

mod common;

use omega_platform::NetworkManager;
use omega_platform::network_manager::{Active, Point, Reading, Scan, Tunnels};
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

    // Local connectivity does not imply internet access.
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
    // Unknown network types map to Unspecified.
    assert_eq!(kind("bluetooth"), NetworkType::Unspecified as i32);
}

#[test]
fn a_vpn_is_reported_as_one_however_it_is_spelled() {
    // The active-connection VPN flag takes precedence over the type string.
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

// ---- the scan ----

fn seen(ssid: &str, strength: u8) -> Point {
    Point {
        ssid: ssid.into(),
        strength,
        flags: 0,
        wpa: 0,
        rsn: 0,
    }
}

#[test]
fn one_network_on_several_radios_is_one_row() {
    // Deduplicate access points by SSID.
    let scan = Scan {
        points: vec![seen("home", 40), seen("home", 71), seen("cafe", 55)],
        active: "home".into(),
    };
    let state = scan.state();

    assert_eq!(state.access_points.len(), 2);
    // Keep the strongest access point for each SSID.
    assert_eq!(state.access_points[0].ssid, "home");
    assert_eq!(state.access_points[0].signal_percent, 71);
}

#[test]
fn the_list_is_strongest_first_and_stable_between_equals() {
    let scan = Scan {
        points: vec![seen("beta", 50), seen("alpha", 50), seen("best", 90)],
        active: String::new(),
    };
    let state = scan.state();
    let names: Vec<&str> = state
        .access_points
        .iter()
        .map(|point| point.ssid.as_str())
        .collect();

    // Break equal-strength ties by SSID for stable ordering.
    assert_eq!(names, vec!["best", "alpha", "beta"]);
}

#[test]
fn security_is_any_of_the_three_ways_networkmanager_says_it() {
    let open = Scan {
        points: vec![seen("open", 50)],
        active: String::new(),
    };
    assert!(!open.state().access_points[0].secured);

    for point in [
        Point {
            flags: 1,
            ..seen("locked", 50)
        },
        Point {
            wpa: 0x100,
            ..seen("locked", 50)
        },
        Point {
            rsn: 392,
            ..seen("locked", 50)
        },
    ] {
        let scan = Scan {
            points: vec![point],
            active: String::new(),
        };
        assert!(scan.state().access_points[0].secured);
    }
}

#[test]
fn the_network_the_machine_is_on_is_marked() {
    let scan = Scan {
        points: vec![seen("home", 70), seen("cafe", 50)],
        active: "home".into(),
    };
    let state = scan.state();

    assert!(state.access_points[0].active);
    assert!(!state.access_points[1].active);
}

#[test]
fn a_hidden_network_is_not_a_row() {
    // Hidden networks broadcast an empty SSID. A row a user cannot tell from
    // another row is not a choice.
    let scan = Scan {
        points: vec![seen("", 90), seen("home", 40)],
        active: String::new(),
    };
    let state = scan.state();

    assert_eq!(state.access_points.len(), 1);
    assert_eq!(state.access_points[0].ssid, "home");
}

// ---- tunnels ----

fn active(id: &str, kind: &str, is_vpn: bool) -> Active {
    Active {
        id: id.into(),
        kind: kind.into(),
        is_vpn,
        interface: "wg0".into(),
    }
}

#[test]
fn a_tunnel_is_spelled_two_ways_and_neither_alone_is_enough() {
    // NetworkManager sets the flag for its VPN plugins and leaves it false
    // for WireGuard, which it drives natively.
    let all = vec![
        active("home wifi", "802-11-wireless", false),
        active("work", "vpn", true),
        active("mullvad", "wireguard", false),
    ];
    let state = Tunnels::state(&all);
    let names: Vec<&str> = state.tunnels.iter().map(|t| t.name.as_str()).collect();

    // Sorted by name, so a bar does not reorder them between readings.
    assert_eq!(names, vec!["mullvad", "work"]);
}

#[test]
fn on_wifi_and_a_vpn_is_two_readings_not_one() {
    // Physical and tunnel connectivity can coexist.
    let all = vec![
        active("home wifi", "802-11-wireless", false),
        active("work", "vpn", true),
    ];
    assert_eq!(Tunnels::state(&all).tunnels.len(), 1);
    // And the `network` topic still reports the link under it separately.
    assert!(!Reading::default().state().connected);
}

// ---- against the machine this is running on ----

#[tokio::test]
#[ignore = "needs a system bus with NetworkManager; run with --ignored"]
async fn it_reads_the_machine_it_is_running_on() {
    let mut network = NetworkManager::new();

    let patch = common::first(&mut network)
        .await
        .expect("NetworkManager answered");
    // Connectivity and scan results publish on separate topics.
    let topics: Vec<&str> = patch
        .topics
        .iter()
        .map(|topic| topic.topic.as_str())
        .collect();
    assert_eq!(topics, vec!["network", "wifi", "vpn"]);

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

    // Subsequent reads wait for a signal or refresh interval.
    assert!(
        common::waits(&mut network).await,
        "a second reading should wait"
    );
}
