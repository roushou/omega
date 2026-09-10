//! How hot the machine is, and what the fans are doing about it.

use crate::reading::Thermals;
use crate::units::Temperature;

/// One temperature sensor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sensor {
    chip: String,
    label: String,
    temperature: Temperature,
}

impl Sensor {
    fn of(sensor: omega_proto::omega::Sensor) -> Self {
        Self {
            temperature: Temperature::of_millicelsius(sensor.millicelsius),
            chip: sensor.chip,
            label: sensor.label,
        }
    }

    /// The chip reporting it: `k10temp`, `thinkpad`, `amdgpu`.
    pub fn chip(&self) -> &str {
        &self.chip
    }

    /// What the chip calls this sensor — `CPU`, `edge`, `Composite` — or its
    /// name in the chip where it has no label.
    pub fn label(&self) -> &str {
        &self.label
    }

    /// Chip and label together, which is the identity: neither is unique on a
    /// machine with eight thermal zones. A list keys rows by this.
    pub fn id(&self) -> String {
        format!("{}/{}", self.chip, self.label)
    }

    /// How hot. Prints itself as `44°C`.
    pub fn temperature(&self) -> Temperature {
        self.temperature
    }
}

/// One fan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fan {
    chip: String,
    label: String,
    rpm: u32,
}

impl Fan {
    fn of(fan: omega_proto::omega::Fan) -> Self {
        Self {
            chip: fan.chip,
            label: fan.label,
            rpm: fan.rpm,
        }
    }

    pub fn chip(&self) -> &str {
        &self.chip
    }

    pub fn label(&self) -> &str {
        &self.label
    }

    pub fn id(&self) -> String {
        format!("{}/{}", self.chip, self.label)
    }

    /// Revolutions per minute. Nought is a fan that is stopped, which is a
    /// quiet machine rather than a missing sensor.
    pub fn rpm(&self) -> u32 {
        self.rpm
    }

    pub fn is_spinning(&self) -> bool {
        self.rpm > 0
    }
}

impl Thermals {
    /// Every sensor that reported something.
    pub fn sensors(&self) -> Vec<Sensor> {
        self.read()
            .map(|state| state.sensors.into_iter().map(Sensor::of).collect())
            .unwrap_or_default()
    }

    /// The hottest thing on the machine — what a bar slot draws when it wants
    /// one number rather than a list of nine.
    pub fn hottest(&self) -> Option<Sensor> {
        self.sensors().into_iter().max_by_key(Sensor::temperature)
    }

    /// Every sensor a given chip reports, for a panel that wants the CPU's
    /// dies together rather than mixed in with the SSD.
    pub fn on(&self, chip: &str) -> Vec<Sensor> {
        self.sensors()
            .into_iter()
            .filter(|sensor| sensor.chip == chip)
            .collect()
    }

    pub fn fans(&self) -> Vec<Fan> {
        self.read()
            .map(|state| state.fans.into_iter().map(Fan::of).collect())
            .unwrap_or_default()
    }

    /// Whether anything is actually turning. A machine with fans reported and
    /// none spinning is a quiet one, which is worth being able to say.
    pub fn is_spinning(&self) -> bool {
        self.fans().iter().any(Fan::is_spinning)
    }
}
