//! Which Bluetooth devices are the machine's, and in what order.

mod common;

use omega_brokers::BlueZ;
use omega_brokers::bluez::{Adapter, Device, Objects};
use omega_proto::omega::state_topic;

fn device(alias: &str, paired: bool, connected: bool) -> Device {
    Device {
        address: "60:AB:D2:25:8C:49".into(),
        alias: alias.into(),
        connected,
        paired,
        icon: "audio-headphones".into(),
        battery: None,
    }
}

fn adapter() -> Adapter {
    Adapter {
        powered: true,
        discovering: false,
    }
}

#[test]
fn only_the_machines_own_devices_are_listed() {
    // BlueZ also lists whatever it has seen recently. A bar showing every
    // phone that walked past is showing the air rather than the machine.
    let seen = vec![
        device("Bose NC 700", true, false),
        device("Somebody's Pixel", false, false),
    ];
    let state = Objects::state(Some(&adapter()), &seen);

    assert_eq!(state.devices.len(), 1);
    assert_eq!(state.devices[0].name, "Bose NC 700");
}

#[test]
fn connected_devices_come_first_and_ties_break_by_name() {
    let seen = vec![
        device("Zed Keyboard", true, false),
        device("Alpha Mouse", true, false),
        device("Bose NC 700", true, true),
    ];
    let state = Objects::state(Some(&adapter()), &seen);
    let names: Vec<&str> = state.devices.iter().map(|d| d.name.as_str()).collect();

    // BlueZ answers in whatever order it holds paths, so a list that did not
    // sort would move under the cursor.
    assert_eq!(names, vec!["Bose NC 700", "Alpha Mouse", "Zed Keyboard"]);
}

#[test]
fn a_machine_with_no_adapter_says_so_rather_than_saying_nothing() {
    // BlueZ answering at all is a reading. A desktop without a radio reports
    // that it has none, which a widget can draw.
    let state = Objects::state(None, &[]);

    assert!(!state.available);
    assert!(!state.powered);
    assert!(state.devices.is_empty());
}

#[test]
fn a_battery_only_exists_on_a_device_that_reports_one() {
    // BlueZ puts `org.bluez.Battery1` on a connected device that has a
    // battery, and nowhere else. Zero means unknown, because a device at zero
    // percent is off and not reporting anything.
    let quiet = Objects::state(Some(&adapter()), &[device("Bose NC 700", true, true)]);
    assert_eq!(quiet.devices[0].battery_percent, 0);

    let reporting = Device {
        battery: Some(72),
        ..device("Bose NC 700", true, true)
    };
    let state = Objects::state(Some(&adapter()), &[reporting]);
    assert_eq!(state.devices[0].battery_percent, 72);
}

// ---- against the machine this is running on ----

#[tokio::test]
#[ignore = "needs a system bus with BlueZ; run with --ignored"]
async fn it_reads_the_machine_it_is_running_on() {
    let mut bluez = BlueZ::new();

    let patch = common::first(&mut bluez).await.expect("BlueZ answered");
    assert_eq!(patch.topics[0].topic, "bluetooth");

    let Some(state_topic::Value::Bluetooth(bluetooth)) = patch.topics[0].value.as_ref() else {
        panic!("expected a bluetooth reading");
    };
    // A machine with no radio is a reading too, so the only claim that always
    // holds is that every listed device is one of this machine's.
    assert!(
        bluetooth
            .devices
            .iter()
            .all(|device| device.paired || device.connected)
    );

    assert!(
        common::waits(&mut bluez).await,
        "a second reading should wait"
    );
}
