//! Combined battery and external power status.

use omega_proto::SystemTopic;
use omega_proto::omega::Capability;

use crate::runtime::context::Context;
use crate::units::{Percent, Remaining};
use crate::wiring::{Reads, Wiring};
use omega_proto::omega::{BatteryState, MainsState};

/// Power source and battery charging status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// The battery is charging.
    Charging,
    /// External power is connected, the battery is not charging, and charge is at least 99%.
    FullyCharged,
    /// External power is connected; the battery is not charging or full.
    OnMains,
    /// The system is using battery power.
    OnBattery,
    /// Available readings do not establish the power source.
    Unknown,
}

impl Status {
    /// Return the default display label.
    pub fn label(self) -> &'static str {
        match self {
            Self::Charging => "Charging",
            Self::FullyCharged => "Fully charged",
            Self::OnMains => "On mains",
            Self::OnBattery => "On battery",
            Self::Unknown => "Unknown",
        }
    }

    /// Whether the system is using battery power.
    pub fn is_draining(self) -> bool {
        self == Self::OnBattery
    }
}

/// Combined battery and external power readings.
/// Subscribes to both topics and updates when either changes.
#[derive(Debug)]
pub struct Power {
    context: Context,
}

impl Wiring for Power {
    const TOPICS: &'static [SystemTopic] = &[SystemTopic::Battery, SystemTopic::Mains];
    const CAPABILITIES: &'static [Capability] = &[Capability::StateRead];

    fn build(context: &Context) -> Self {
        Self {
            context: context.clone(),
        }
    }
}

impl Reads for Power {}

impl Power {
    /// Treat a non-charging battery at 99% or higher as fully charged.
    const FULL: u8 = 99;

    /// Whether a battery reading is available.
    pub fn has_battery(&self) -> bool {
        self.context.read().get::<BatteryState>().is_some()
    }

    /// Whether external power is connected.
    pub fn on_mains(&self) -> bool {
        self.context
            .read()
            .get::<MainsState>()
            .is_some_and(|mains| mains.connected)
    }

    pub fn is_charging(&self) -> bool {
        self.context
            .read()
            .get::<BatteryState>()
            .is_some_and(|battery| battery.charging)
    }

    /// Battery charge, or `None` when no battery reading is available.
    pub fn charge(&self) -> Option<Percent> {
        self.context
            .read()
            .get::<BatteryState>()
            .map(|battery| Percent::of(battery.level))
    }

    /// Estimated time until full or empty, according to charging state.
    pub fn remaining(&self) -> Option<Remaining> {
        let state = self.context.read();
        let battery = state.get::<BatteryState>()?;
        Remaining::seconds(if battery.charging {
            battery.seconds_to_full
        } else {
            battery.seconds_to_empty
        })
    }

    /// Interpret the battery and mains from one replicated snapshot.
    /// Missing mains data never implies that the cable is disconnected.
    ///
    /// ```no_run
    /// # fn example(power: &omega::platform::power::Power) {
    /// if power.status() == omega::platform::power::Status::Unknown {
    ///     // Show an unavailable state rather than claiming battery operation.
    /// }
    /// # }
    /// ```
    pub fn status(&self) -> Status {
        let state = self.context.read();
        let battery = state.get::<BatteryState>();
        let mains = state.get::<MainsState>();
        if battery.is_some_and(|battery| battery.charging) {
            return Status::Charging;
        }
        match mains.map(|mains| mains.connected) {
            Some(true)
                if battery.is_some_and(|battery| Percent::of(battery.level) >= Self::FULL) =>
            {
                Status::FullyCharged
            }
            Some(true) => Status::OnMains,
            Some(false) if battery.is_some() => Status::OnBattery,
            _ => Status::Unknown,
        }
    }
}
