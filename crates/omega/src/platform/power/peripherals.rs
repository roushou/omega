//! Peripheral battery readings.

crate::wiring::reading! {
    /// Peripheral battery readings.
    Peripherals: omega_proto::omega::PeripheralsState
}

use crate::units::Percent;

pub use omega_proto::omega::PeripheralKind;

/// A battery-powered peripheral, such as a mouse, headset, or phone.
#[derive(Debug, Clone, PartialEq)]
pub struct Peripheral {
    id: String,
    model: String,
    kind: PeripheralKind,
    charge: Percent,
    charging: bool,
}

impl Peripheral {
    fn of(device: omega_proto::omega::Peripheral) -> Self {
        Self {
            kind: PeripheralKind::try_from(device.kind).unwrap_or(PeripheralKind::Unspecified),
            // Clamp the reported percentage to 0–100.
            charge: Percent::whole(device.percent.min(100) as u8),
            id: device.id,
            model: device.model,
            charging: device.charging,
        }
    }

    /// Device identifier, stable across reconnects and suitable for item keys.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Device model name, such as `Logitech G Pro`.
    pub fn model(&self) -> &str {
        &self.model
    }

    pub fn kind(&self) -> PeripheralKind {
        self.kind
    }

    /// Battery charge as a percentage.
    pub fn charge(&self) -> Percent {
        self.charge
    }

    pub fn is_charging(&self) -> bool {
        self.charging
    }
}

impl Peripherals {
    /// Return peripheral batteries in ascending charge order.
    pub fn devices(&self) -> Vec<Peripheral> {
        self.read()
            .map(|state| state.devices.into_iter().map(Peripheral::of).collect())
            .unwrap_or_default()
    }
}
