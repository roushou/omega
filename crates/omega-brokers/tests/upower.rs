//! Turning UPower's numbers into the ontology.
//!
//! The connection is not testable without a bus; this is. What UPower reports
//! and what a widget draws are different vocabularies — percent against
//! fraction, a state enum against a boolean, and two time fields of which
//! only one ever applies — and every one of those is somewhere to be wrong.

mod common;

use omega_brokers::upower::{Attached, Peripherals, Reading};

/// A laptop battery, discharging with two hours left.
fn discharging() -> Reading {
    Reading {
        is_present: true,
        kind: 2,
        state: 2,
        percentage: 80.0,
        time_to_empty: 7200,
        time_to_full: 0,
    }
}

#[test]
fn a_percentage_becomes_a_fraction() {
    // The wire carries 0.0 .. 1.0 and UPower speaks percent. A broker that
    // passed the number through would report a battery at 8000%.
    let state = discharging().state().expect("a battery is present");
    assert_eq!(state.level, 0.8);
    assert!(!state.charging);
    assert_eq!(state.seconds_to_empty, 7200);
}

#[test]
fn only_the_time_that_applies_is_reported() {
    let charging = Reading {
        state: 1,
        time_to_empty: 0,
        time_to_full: 1800,
        ..discharging()
    };
    let state = charging.state().expect("a battery is present");
    assert!(state.charging);
    assert_eq!(state.seconds_to_full, 1800);
    assert_eq!(
        state.seconds_to_empty, 0,
        "a battery being charged is not also emptying"
    );

    // And the other way: UPower leaves the inapplicable one at whatever it
    // last was, so passing both through would show a full-in-30-minutes on a
    // battery nothing is charging.
    let stale = Reading {
        time_to_full: 1800,
        ..discharging()
    };
    assert_eq!(stale.state().unwrap().seconds_to_full, 0);
}

#[test]
fn a_full_battery_on_ac_is_not_charging() {
    // UpDeviceState::FullyCharged. A widget told this was "charging" would
    // say so for as long as the machine stayed plugged in.
    let full = Reading {
        state: 4,
        percentage: 100.0,
        ..discharging()
    };
    let state = full.state().expect("a battery is present");
    assert_eq!(state.level, 1.0);
    assert!(!state.charging);
}

#[test]
fn a_machine_with_no_battery_has_no_reading() {
    // Two ways UPower says it, and neither is an error: a desktop reports a
    // display device that is not a battery, and a laptop with the pack out
    // reports a battery that is not present.
    assert!(
        Reading {
            kind: 0,
            ..discharging()
        }
        .state()
        .is_none()
    );
    assert!(
        Reading {
            is_present: false,
            ..discharging()
        }
        .state()
        .is_none()
    );
}

#[test]
fn an_estimate_still_being_worked_out_is_not_a_negative_duration() {
    // UPower's seconds are signed and briefly negative after a state change.
    // The ontology's are unsigned, and casting would report seventy years.
    let settling = Reading {
        time_to_empty: -1,
        ..discharging()
    };
    assert_eq!(settling.state().unwrap().seconds_to_empty, 0);
}

// ---- what is plugged in ----

fn attached(path: &str, kind: u32, percentage: f64) -> Attached {
    Attached {
        path: format!("/org/freedesktop/UPower/devices/{path}"),
        model: "Logitech G Pro".into(),
        kind,
        percentage,
        state: 0,
    }
}

#[test]
fn the_machines_own_supplies_are_not_peripherals() {
    // UPower reports the laptop battery, the mains, and the mouse through one
    // interface. The first two are what `battery` and `power` already answer.
    let all = vec![
        attached("battery_BAT0", 2, 87.0),
        attached("line_power_AC", 1, 0.0),
        attached("battery_hidpp_battery_2", 5, 72.0),
    ];
    let state = Peripherals::state(&all);

    assert_eq!(state.devices.len(), 1);
    assert_eq!(state.devices[0].percent, 72);
}

#[test]
fn a_device_that_reports_no_battery_is_not_a_device_at_zero() {
    // A keyboard with no battery is a keyboard, and listing it as empty would
    // have a bar warning about hardware that is fine.
    assert!(
        Peripherals::state(&[attached("kbd", 6, 0.0)])
            .devices
            .is_empty()
    );
}

#[test]
fn the_emptiest_comes_first_and_ties_break_by_model() {
    let all = vec![
        attached("a", 5, 90.0),
        attached("b", 6, 12.0),
        attached("c", 17, 50.0),
    ];
    let percents: Vec<u32> = Peripherals::state(&all)
        .devices
        .iter()
        .map(|device| device.percent)
        .collect();

    // A bar wants the one about to die at the top, and a stable order under
    // it so the list does not shuffle between readings.
    assert_eq!(percents, vec![12, 50, 90]);
}

#[test]
fn the_id_is_the_path_because_a_model_is_not_unique() {
    // Two identical mice are two devices. The object path is what survives a
    // reconnect and tells them apart.
    let state = Peripherals::state(&[attached("battery_hidpp_battery_2", 5, 72.0)]);
    assert_eq!(state.devices[0].id, "battery_hidpp_battery_2");
}

// ---- against the machine this is running on ----

use omega_brokers::UPower;
use omega_proto::omega::state_topic;

#[tokio::test]
#[ignore = "needs a system bus with UPower; run with --ignored"]
async fn it_reads_the_machine_it_is_running_on() {
    let mut upower = UPower::new();

    // A fresh connection reports what is true now. Waiting for the next
    // change instead would leave a widget blank until the battery moved,
    // which on a machine sitting on AC is never.
    let patch = common::first(&mut upower).await.expect("UPower answered");
    // One power supply subsystem, one broker, two topics: the battery and
    // whether the machine is on mains.
    let topics: Vec<&str> = patch
        .topics
        .iter()
        .map(|topic| topic.topic.as_str())
        .collect();
    assert_eq!(topics, vec!["battery", "power", "peripherals"]);

    // A desktop reports no battery and is still on mains, so `power` always
    // has a value where `battery` may not.
    assert!(patch.topics[1].value.is_some());

    match patch.topics[0].value.as_ref() {
        Some(state_topic::Value::Battery(battery)) => {
            assert!((0.0..=1.0).contains(&battery.level), "{battery:?}");
        }
        // A desktop. The topic is still published, which is the point.
        None => {}
        other => panic!("expected a battery, got {other:?}"),
    }

    // The second reading waits for UPower to say something changed. A broker
    // that answered again straight away would be a hot loop pretending to be
    // signal-driven.
    assert!(
        common::waits(&mut upower).await,
        "a second reading should wait"
    );
}
