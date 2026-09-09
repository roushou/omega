//! The batteries of things plugged into the machine.

use crate::state::Peripherals;
use crate::units::Percent;

pub use omega_proto::omega::PeripheralKind;

/// One thing with a battery of its own — a mouse, a headset, a phone.
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
            // Clamped once, here, rather than at every call site: a source
            // reporting 130% is wrong and a bar drawn that wide is worse.
            charge: Percent::whole(device.percent.min(100) as u8),
            id: device.id,
            model: device.model,
            charging: device.charging,
        }
    }

    /// Stable across a disconnect, which is what a list keys its rows by so a
    /// mouse that sleeps and wakes keeps its row.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// What it calls itself: `Logitech G Pro`.
    pub fn model(&self) -> &str {
        &self.model
    }

    pub fn kind(&self) -> PeripheralKind {
        self.kind
    }

    /// How full it is. Prints itself as `66%`.
    pub fn charge(&self) -> Percent {
        self.charge
    }

    pub fn is_charging(&self) -> bool {
        self.charging
    }
}

impl Peripherals {
    /// Everything with a battery, emptiest first — the order UPower's broker
    /// already sorted them into.
    pub fn devices(&self) -> Vec<Peripheral> {
        self.read()
            .map(|state| state.devices.into_iter().map(Peripheral::of).collect())
            .unwrap_or_default()
    }
}
