//! The adapter, and what is paired with it.

use crate::state::Bluetooth;
use crate::units::Percent;

/// One paired or nearby device.
#[derive(Debug, Clone, PartialEq)]
pub struct BluetoothDevice {
    address: String,
    name: String,
    connected: bool,
    paired: bool,
    icon: String,
    battery: Option<Percent>,
}

impl BluetoothDevice {
    fn of(device: omega_proto::omega::BluetoothDevice) -> Self {
        Self {
            // Nought is how BlueZ spells "this device does not report one",
            // which is not the same as a flat battery.
            battery: match device.battery_percent {
                0 => None,
                percent => Some(Percent::whole(percent.min(100) as u8)),
            },
            address: device.address,
            name: device.name,
            connected: device.connected,
            paired: device.paired,
            icon: device.icon,
        }
    }

    /// `60:AB:D2:25:8C:49` — its identity, and what a list keys rows by.
    pub fn address(&self) -> &str {
        &self.address
    }

    /// The alias, which is what a person renamed it to.
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn is_connected(&self) -> bool {
        self.connected
    }

    pub fn is_paired(&self) -> bool {
        self.paired
    }

    /// BlueZ's own icon name — `audio-headphones`. Not one of this shell's
    /// glyph names: it is a freedesktop icon name, from a different set.
    pub fn icon(&self) -> &str {
        &self.icon
    }

    /// Its charge, or `None` for a device that does not report one.
    pub fn battery(&self) -> Option<Percent> {
        self.battery
    }
}

impl Bluetooth {
    /// Whether the machine has an adapter at all.
    pub fn is_available(&self) -> bool {
        self.read().is_some_and(|state| state.available)
    }

    pub fn is_powered(&self) -> bool {
        self.read().is_some_and(|state| state.powered)
    }

    pub fn is_discovering(&self) -> bool {
        self.read().is_some_and(|state| state.discovering)
    }

    pub fn devices(&self) -> Vec<BluetoothDevice> {
        self.read()
            .map(|state| state.devices.into_iter().map(BluetoothDevice::of).collect())
            .unwrap_or_default()
    }

    /// Only what is connected right now — what a bar slot counts.
    pub fn connected(&self) -> Vec<BluetoothDevice> {
        self.devices()
            .into_iter()
            .filter(BluetoothDevice::is_connected)
            .collect()
    }
}
