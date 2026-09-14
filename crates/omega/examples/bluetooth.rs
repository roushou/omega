//! Known Bluetooth devices and their connection controls.
use omega::platform::bluetooth::{
    Bluetooth, BluetoothControl, BluetoothDevice, BluetoothStatus, DeviceId,
};
use omega::ui::{Button, Column, Glyph, Icon, Row, Section, Stack, Text};
use omega::{Command, Plugin, Surface, Ui};

#[derive(omega::Surface, Debug)]
pub struct Indicator {
    bluetooth: Bluetooth,
}
impl Surface for Indicator {
    fn render(&self) -> Ui {
        let tooltip = match self.bluetooth.status() {
            BluetoothStatus::Unavailable => "Bluetooth state unavailable",
            BluetoothStatus::NoAdapter => "No Bluetooth adapter",
            BluetoothStatus::Off => "Bluetooth is off",
            BluetoothStatus::On => return self.connected_indicator(),
        };
        Icon::new(Glyph::Bluetooth).muted().tooltip(tooltip).into()
    }
}
impl Indicator {
    fn connected_indicator(&self) -> Ui {
        let devices = self.bluetooth.connected_devices();
        let mut indicator = Row::new().gap(6).child(Icon::new(Glyph::Bluetooth));
        if !devices.is_empty() {
            indicator = indicator.child(Text::new(devices.len()));
        }
        let tooltip = if devices.is_empty() {
            "No connected devices".to_string()
        } else {
            devices
                .iter()
                .map(|device| match device.battery() {
                    Some(battery) => format!("{} — {battery}", Panel::name(device)),
                    None => Panel::name(device).to_string(),
                })
                .collect::<Vec<_>>()
                .join("\n")
        };
        indicator.tooltip(tooltip).into()
    }
}

#[derive(omega::Surface, Debug)]
pub struct Panel {
    bluetooth: Bluetooth,
}
impl Panel {
    fn name(device: &BluetoothDevice) -> &str {
        if device.name().is_empty() {
            device.address()
        } else {
            device.name()
        }
    }

    fn device(device: BluetoothDevice) -> Stack {
        let mut heading = Row::new()
            .gap(8)
            .child(Text::new(Self::name(&device)).bold().fill_width());
        if let Some(battery) = device.battery() {
            heading = heading.child(Text::new(battery));
        }
        let status = if device.is_connected() {
            "Connected"
        } else if device.can_connect() {
            "Disconnected"
        } else {
            "Unavailable — Bluetooth is off or this device is blocked"
        };
        let button = if device.is_connected() {
            Button::new("Disconnect").on_press(Disconnect.with(device.id().clone()))
        } else {
            Button::new("Connect")
                .disabled_if(!device.can_connect())
                .on_press(Connect.with(device.id().clone()))
        };
        Column::new()
            .gap(6)
            .key(device.id().as_str())
            .child(heading)
            .child(Text::new(status).muted())
            .child(button.fill_width())
    }
}
impl Surface for Panel {
    fn render(&self) -> Ui {
        let panel = Section::new("Bluetooth");
        let panel = match self.bluetooth.status() {
            BluetoothStatus::Unavailable => {
                return panel.child(Text::new("Bluetooth state unavailable")).into();
            }
            BluetoothStatus::NoAdapter => {
                return panel.child(Text::new("No Bluetooth adapter")).into();
            }
            BluetoothStatus::Off => panel.child(Text::new(
                "Bluetooth is off. Turn it on in Bluetooth settings.",
            )),
            BluetoothStatus::On => panel,
        };
        let devices = self.bluetooth.known_devices();
        if devices.is_empty() {
            return panel
                .child(Text::new("No known devices"))
                .child(Text::new("Pair a device in Bluetooth settings to use it here.").muted())
                .into();
        }
        panel.children(devices.into_iter().map(Self::device)).into()
    }
}

#[derive(omega::Command, Debug)]
pub struct Connect {
    bluetooth: BluetoothControl,
}
impl Command for Connect {
    type Input = DeviceId;
    type Output = ();
    async fn call(&self, id: DeviceId) -> omega::Result<()> {
        self.bluetooth.connect(&id).await
    }
}
#[derive(omega::Command, Debug)]
pub struct Disconnect {
    bluetooth: BluetoothControl,
}
impl Command for Disconnect {
    type Input = DeviceId;
    type Output = ();
    async fn call(&self, id: DeviceId) -> omega::Result<()> {
        self.bluetooth.disconnect(&id).await
    }
}

fn main() -> omega::Result<()> {
    Plugin::named(env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"))
        .surface_as::<Indicator>("indicator")
        .surface_as::<Panel>("panel")
        .command::<Connect>()
        .command::<Disconnect>()
        .run()
}

#[cfg(test)]
mod tests {
    use super::*;
    use omega::testing::{Called, Drawn, State, SystemTopic, manifest_of, topic::BluetoothState};
    use omega_proto::{
        IntoValue,
        omega::{
            BluetoothDevice as WireDevice, Capability, ConnectBluetooth, DisconnectBluetooth,
            action,
        },
    };

    struct Fixture;
    impl Fixture {
        const ID: &'static str = "/org/bluez/hci0/dev_60_AB_D2_25_8C_49";
        fn device() -> WireDevice {
            WireDevice {
                id: Self::ID.into(),
                address: "60:AB:D2:25:8C:49".into(),
                name: "Headphones".into(),
                paired: true,
                can_connect: true,
                ..Default::default()
            }
        }
        fn state(device: WireDevice) -> State {
            State::new().with(BluetoothState {
                available: true,
                powered: true,
                devices: vec![device],
                ..Default::default()
            })
        }
    }

    #[test]
    fn absence_power_and_empty_devices_are_distinct() {
        assert!(
            Drawn::of::<Panel>(&State::new().absent(SystemTopic::Bluetooth))
                .text()
                .contains("state unavailable")
        );
        assert!(
            Drawn::of::<Panel>(&State::new().with(BluetoothState::default()))
                .text()
                .contains("No Bluetooth adapter")
        );
        let state = State::new().with(BluetoothState {
            available: true,
            ..Default::default()
        });
        let text = Drawn::of::<Panel>(&state).text();
        assert!(text.contains("Bluetooth is off"));
        assert!(text.contains("No known devices"));
    }

    #[test]
    fn indicator_distinguishes_reading_adapter_and_power() {
        for (state, tooltip) in [
            (
                State::new().absent(SystemTopic::Bluetooth),
                "Bluetooth state unavailable",
            ),
            (
                State::new().with(BluetoothState {
                    powered: true,
                    ..Default::default()
                }),
                "No Bluetooth adapter",
            ),
            (
                State::new().with(BluetoothState {
                    available: true,
                    ..Default::default()
                }),
                "Bluetooth is off",
            ),
            (
                State::new().with(BluetoothState {
                    available: true,
                    powered: true,
                    ..Default::default()
                }),
                "No connected devices",
            ),
        ] {
            let drawn = Drawn::of::<Indicator>(&state);
            assert_eq!(drawn.prop("root", "tooltip").as_deref(), Some(tooltip));
        }
    }

    #[test]
    fn zero_battery_is_drawn_and_disconnected_devices_can_connect() {
        let device = WireDevice {
            battery_percent: Some(0),
            ..Fixture::device()
        };
        let state = Fixture::state(device);
        let panel = Drawn::of::<Panel>(&state);
        assert!(panel.text().contains("0%"));
        assert!(panel.text().contains("Disconnected"));
        assert_eq!(
            panel
                .prop(&panel.first("button").unwrap(), "label")
                .as_deref(),
            Some("Connect")
        );
        let row = panel.node(Fixture::ID).unwrap();
        let button = row.children.last().unwrap();
        assert_eq!(button.props["disabled"], false.into_value());
        let bind = &button.events["press"];
        assert_eq!(bind.args, vec![Fixture::ID.into_value()]);
    }

    #[test]
    fn unknown_battery_is_omitted_and_connected_devices_can_disconnect() {
        let state = Fixture::state(WireDevice {
            connected: true,
            ..Fixture::device()
        });
        let text = Drawn::of::<Panel>(&state).text();
        assert!(!text.contains('%'));
        let panel = Drawn::of::<Panel>(&state);
        assert_eq!(
            panel
                .prop(&panel.first("button").unwrap(), "label")
                .as_deref(),
            Some("Disconnect")
        );
        assert!(Drawn::of::<Indicator>(&state).text().contains('1'));
        let state = Fixture::state(WireDevice {
            can_connect: false,
            ..Fixture::device()
        });
        let panel = Drawn::of::<Panel>(&state);
        assert_eq!(
            panel
                .node(Fixture::ID)
                .unwrap()
                .children
                .last()
                .unwrap()
                .props["disabled"],
            true.into_value()
        );
    }

    #[tokio::test]
    async fn controls_route_typed_ids_and_reject_invalid_input_before_effects() {
        let id = DeviceId::parse(Fixture::ID).unwrap();
        let called = Called::of::<Connect>(&State::new(), id.clone()).await;
        assert!(called.answer.is_ok());
        assert!(
            called.did(&action::Kind::ConnectBluetooth(ConnectBluetooth {
                device_id: id.to_string()
            }))
        );
        let called = Called::of::<Disconnect>(&State::new(), id.clone()).await;
        assert!(called.answer.is_ok());
        assert!(
            called.did(&action::Kind::DisconnectBluetooth(DisconnectBluetooth {
                device_id: id.to_string()
            }))
        );
        let called =
            Called::raw::<Connect>(&State::new(), vec!["/org/bluez/hci0".into_value()]).await;
        assert!(called.answer.is_err());
        assert!(called.effects.is_empty());
        let manifest = manifest_of(
            &Plugin::named("bluetooth", "0.1.0")
                .surface_as::<Panel>("bluetooth")
                .command::<Connect>()
                .command::<Disconnect>(),
        );
        assert!(manifest.granted().unwrap().contains(&Capability::Bluetooth));
        assert!(!manifest.granted().unwrap().contains(&Capability::Spawn));
    }
}
