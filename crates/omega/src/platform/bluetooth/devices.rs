//! Bluetooth adapter state and paired or connected devices.

crate::wiring::reading! {
    /// Bluetooth adapters and paired or connected devices.
    Bluetooth: omega_proto::omega::BluetoothState
}

use crate::units::Percent;

/// Availability of the current Bluetooth reading and its adapters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BluetoothStatus {
    /// No current reading, including while the broker is initializing.
    Unavailable,
    /// A reading exists, but no Bluetooth adapter is present.
    NoAdapter,
    /// Adapters are present, but none is powered.
    Off,
    /// At least one adapter is powered; devices may still be disconnected.
    On,
}

/// One paired or connected device.
#[derive(Debug, Clone, PartialEq)]
pub struct BluetoothDevice {
    id: omega_proto::BluetoothDeviceId,
    can_connect: bool,
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
            id: omega_proto::BluetoothDeviceId::try_from(device.id)
                .expect("daemon supplied a valid Bluetooth device id"),
            can_connect: device.can_connect,
            battery: device
                .battery_percent
                .map(|percent| Percent::whole(percent.min(100) as u8)),
            address: device.address,
            name: device.name,
            connected: device.connected,
            paired: device.paired,
            icon: device.icon,
        }
    }

    /// The adapter-qualified identity to bind to a connect/disconnect command.
    pub fn id(&self) -> &omega_proto::BluetoothDeviceId {
        &self.id
    }

    /// Whether the device is paired, unblocked, and its adapter is powered.
    pub fn can_connect(&self) -> bool {
        self.can_connect
    }

    /// `60:AB:D2:25:8C:49` — its radio address. Use `id()` for keys and controls.
    pub fn address(&self) -> &str {
        &self.address
    }

    /// Device alias reported by BlueZ.
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn is_connected(&self) -> bool {
        self.connected
    }

    pub fn is_paired(&self) -> bool {
        self.paired
    }

    /// Freedesktop icon name reported by BlueZ, such as `audio-headphones`.
    /// Use with `Image::icon`, not `Icon::named`.
    pub fn icon(&self) -> &str {
        &self.icon
    }

    /// Its charge, or `None` for a device that does not report one.
    pub fn battery(&self) -> Option<Percent> {
        self.battery
    }
}

impl Bluetooth {
    /// Interpret the current reading once, from availability through power.
    ///
    /// ```
    /// use omega::{platform::bluetooth::{Bluetooth, BluetoothStatus}, View, Surface};
    /// use omega::ui::Text;
    /// #[derive(omega::Surface)]
    /// struct Indicator { bluetooth: Bluetooth }
    /// impl Surface for Indicator {
    ///     type Model = ();
    ///     type Message = std::convert::Infallible;
    ///     type Effects = ();
    ///     fn update(&self, _: &mut (), message: Self::Message, _: &()) -> omega::surface::Task<Self::Message> {
    ///         match message {}
    ///     }
    ///
    ///     fn render(&self, _: &(), _: &omega::surface::Events<Self::Message>) -> View {
    ///         Text::new(match self.bluetooth.status() {
    ///             BluetoothStatus::Unavailable => "Bluetooth state unavailable",
    ///             BluetoothStatus::NoAdapter => "No Bluetooth adapter",
    ///             BluetoothStatus::Off => "Bluetooth is off",
    ///             BluetoothStatus::On => "Bluetooth is on",
    ///         }).into()
    ///     }
    /// }
    /// ```
    pub fn status(&self) -> BluetoothStatus {
        match self.read() {
            None => BluetoothStatus::Unavailable,
            Some(state) if !state.available => BluetoothStatus::NoAdapter,
            Some(state) if !state.powered => BluetoothStatus::Off,
            Some(_) => BluetoothStatus::On,
        }
    }

    /// Whether at least one Bluetooth adapter is present. Returns `false` without a reading.
    pub fn is_available(&self) -> bool {
        self.read().is_some_and(|state| state.available)
    }

    pub fn is_powered(&self) -> bool {
        self.read().is_some_and(|state| state.powered)
    }

    pub fn is_discovering(&self) -> bool {
        self.read().is_some_and(|state| state.discovering)
    }

    /// Paired or connected devices, including disconnected devices available to reconnect.
    pub fn known_devices(&self) -> Vec<BluetoothDevice> {
        self.read()
            .map(|state| state.devices.into_iter().map(BluetoothDevice::of).collect())
            .unwrap_or_default()
    }

    /// Return the currently connected devices.
    pub fn connected_devices(&self) -> Vec<BluetoothDevice> {
        self.known_devices()
            .into_iter()
            .filter(BluetoothDevice::is_connected)
            .collect()
    }
}
