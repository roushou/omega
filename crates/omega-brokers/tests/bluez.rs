//! Which Bluetooth devices are the machine's, and in what order.

mod common;

use omega_brokers::BlueZ;
use omega_brokers::bluez::{Adapter, Device, Objects};
use omega_proto::omega::state_topic;

fn device(alias: &str, paired: bool, connected: bool) -> Device {
    Device {
        id: omega_proto::BluetoothDeviceId::parse("/org/bluez/hci0/dev_60_AB_D2_25_8C_49").unwrap(),
        can_connect: paired,
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
    let quiet = Objects::state(Some(&adapter()), &[device("Bose NC 700", true, true)]);
    assert_eq!(quiet.devices[0].battery_percent, None);

    let reporting = Device {
        battery: Some(72),
        ..device("Bose NC 700", true, true)
    };
    let state = Objects::state(Some(&adapter()), &[reporting]);
    assert_eq!(state.devices[0].battery_percent, Some(72));
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

#[test]
fn empty_battery_is_distinct_from_missing_and_targets_include_the_adapter() {
    let first = Device {
        battery: Some(0),
        ..device("Headphones", true, false)
    };
    let second = Device {
        id: omega_proto::BluetoothDeviceId::parse("/org/bluez/hci1/dev_60_AB_D2_25_8C_49").unwrap(),
        ..first.clone()
    };
    let target = second.id.clone();
    let devices = vec![first, second];
    assert_eq!(
        Objects::state(Some(&adapter()), &devices).devices[0].battery_percent,
        Some(0)
    );
    assert_eq!(Objects::target(&devices, &target, true).unwrap().id, target);
    assert!(Objects::target(&devices[..1], &target, true).is_err());
    let unavailable = vec![Device {
        can_connect: false,
        ..devices[1].clone()
    }];
    assert!(Objects::target(&unavailable, &target, true).is_err());
    assert!(Objects::target(&unavailable, &target, false).is_ok());
    let nearby = vec![Device {
        paired: false,
        connected: false,
        ..devices[1].clone()
    }];
    assert!(Objects::target(&nearby, &target, false).is_err());
}

mod isolated {
    use super::*;
    use omega_brokers::Broker;
    use omega_proto::omega::{ConnectBluetooth, DisconnectBluetooth, action};
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    use zbus::zvariant::OwnedObjectPath;

    struct Radio;
    #[zbus::interface(name = "org.bluez.Adapter1")]
    impl Radio {
        #[zbus(property)]
        fn powered(&self) -> bool {
            true
        }
        #[zbus(property)]
        fn discovering(&self) -> bool {
            false
        }
    }
    struct Remote {
        adapter: OwnedObjectPath,
        calls: Arc<AtomicUsize>,
    }
    #[zbus::interface(name = "org.bluez.Device1")]
    impl Remote {
        fn connect(&self) {
            self.calls.fetch_add(1, Ordering::SeqCst);
        }
        fn disconnect(&self) -> zbus::fdo::Result<()> {
            Err(zbus::fdo::Error::Failed("device refused disconnect".into()))
        }
        #[zbus(property)]
        fn adapter(&self) -> OwnedObjectPath {
            self.adapter.clone()
        }
        #[zbus(property)]
        fn alias(&self) -> &str {
            "Headphones"
        }
        #[zbus(property)]
        fn address(&self) -> &str {
            "60:AB:D2:25:8C:49"
        }
        #[zbus(property)]
        fn paired(&self) -> bool {
            true
        }
        #[zbus(property)]
        fn connected(&self) -> bool {
            false
        }
        #[zbus(property)]
        fn blocked(&self) -> bool {
            false
        }
    }
    struct Battery;
    #[zbus::interface(name = "org.bluez.Battery1")]
    impl Battery {
        #[zbus(property)]
        fn percentage(&self) -> u8 {
            0
        }
    }

    #[tokio::test]
    #[ignore = "requires an isolated bus with DBUS_SYSTEM_BUS_ADDRESS pointing at it"]
    async fn isolated_routes_only_the_selected_adapter_and_reports_errors() {
        let first = Arc::new(AtomicUsize::new(0));
        let second = Arc::new(AtomicUsize::new(0));
        let first_path = "/org/bluez/hci0/dev_60_AB_D2_25_8C_49";
        let second_path = "/org/bluez/hci1/dev_60_AB_D2_25_8C_49";
        let server = zbus::connection::Builder::system()
            .unwrap()
            .name("org.bluez")
            .unwrap()
            .serve_at("/", zbus::fdo::ObjectManager)
            .unwrap()
            .serve_at("/org/bluez/hci0", Radio)
            .unwrap()
            .serve_at("/org/bluez/hci1", Radio)
            .unwrap()
            .serve_at(
                first_path,
                Remote {
                    adapter: "/org/bluez/hci0".try_into().unwrap(),
                    calls: first.clone(),
                },
            )
            .unwrap()
            .serve_at(
                second_path,
                Remote {
                    adapter: "/org/bluez/hci1".try_into().unwrap(),
                    calls: second.clone(),
                },
            )
            .unwrap()
            .serve_at(second_path, Battery)
            .unwrap()
            .build()
            .await
            .unwrap();
        let mut broker = BlueZ::new();
        broker.connect().await.unwrap();
        let patch = broker.read().await.unwrap();
        let Some(state_topic::Value::Bluetooth(state)) = &patch.topics[0].value else {
            panic!("Bluetooth reading")
        };
        assert_eq!(state.devices.len(), 2);
        assert!(state.devices.iter().all(|device| device.can_connect));
        assert_eq!(
            state
                .devices
                .iter()
                .find(|device| device.id == second_path)
                .unwrap()
                .battery_percent,
            Some(0)
        );
        broker
            .act(&action::Kind::ConnectBluetooth(ConnectBluetooth {
                device_id: second_path.into(),
            }))
            .await
            .unwrap();
        assert_eq!(first.load(Ordering::SeqCst), 0);
        assert_eq!(second.load(Ordering::SeqCst), 1);
        let error = broker
            .act(&action::Kind::DisconnectBluetooth(DisconnectBluetooth {
                device_id: second_path.into(),
            }))
            .await
            .unwrap_err();
        assert!(error.to_string().contains("device refused disconnect"));
        server
            .object_server()
            .remove::<Remote, _>(second_path)
            .await
            .unwrap();
        assert!(
            broker
                .act(&action::Kind::ConnectBluetooth(ConnectBluetooth {
                    device_id: second_path.into()
                }))
                .await
                .is_err()
        );
        assert_eq!(first.load(Ordering::SeqCst), 0);
        assert_eq!(second.load(Ordering::SeqCst), 1);
    }
}
