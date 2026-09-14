//! Temperature sensors and fan speeds.

crate::wiring::reading! {
    /// Temperature sensor readings and fan speeds.
    Thermals: omega_proto::omega::ThermalsState
}

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

    /// Sensor label, falling back to its name within the chip.
    pub fn label(&self) -> &str {
        &self.label
    }

    /// Combined chip and sensor label identifier, suitable for item keys.
    pub fn id(&self) -> String {
        format!("{}/{}", self.chip, self.label)
    }

    /// Sensor temperature.
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

    /// Fan speed in revolutions per minute. Zero indicates a stopped fan.
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

    /// Return the sensor with the highest reported temperature.
    pub fn hottest(&self) -> Option<Sensor> {
        self.sensors().into_iter().max_by_key(Sensor::temperature)
    }

    /// Return sensors reported by the named chip.
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

    /// Whether any reported fan has a nonzero speed.
    pub fn is_spinning(&self) -> bool {
        self.fans().iter().any(Fan::is_spinning)
    }
}
