//! What the machine is doing about power.

use omega_proto::SystemTopic;
use omega_proto::omega::Capability;

use crate::context::Context;
use crate::units::{Percent, Remaining};
use crate::wiring::{Reads, Wiring};
use omega_proto::omega::{BatteryState, MainsState};

/// The machine's power situation, in one word.
///
/// An enum rather than a string because these are states and not
/// sentences: a widget may want to colour them, and a shell may want to
/// translate them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// The charge is going up.
    Charging,
    /// On the wall, and there is nothing left to put in.
    FullyCharged,
    /// On the wall, and not charging — which is not the same as full: a
    /// machine can be held at 80% on purpose.
    OnMains,
    /// Running down.
    OnBattery,
    /// Available readings do not establish the power source.
    Unknown,
}

impl Status {
    /// What a bar would write. Separate from the enum so a unit can branch on
    /// the state and still get the wording for free.
    pub fn label(self) -> &'static str {
        match self {
            Self::Charging => "Charging",
            Self::FullyCharged => "Fully charged",
            Self::OnMains => "On mains",
            Self::OnBattery => "On battery",
            Self::Unknown => "Unknown",
        }
    }

    /// Whether this is a state somebody should do something about.
    pub fn is_draining(self) -> bool {
        self == Self::OnBattery
    }
}

/// The battery and the socket, read together.
///
/// Hold this instead of both when the question spans them. Holding `Battery`
/// alone is still right for a widget that only draws a charge: this declares
/// two topics and so wakes on either.
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
    /// At what charge a battery on the wall counts as full.
    ///
    /// Not 100: a machine that stops charging at 99 to spare the cell would
    /// otherwise read as "on mains" forever, which is true and unhelpful.
    const FULL: u8 = 99;

    /// Whether this machine has a battery at all. False on a desktop, and
    /// false while the broker that reports it is down.
    pub fn has_battery(&self) -> bool {
        self.context.read().get::<BatteryState>().is_some()
    }

    /// Whether the cable is in.
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

    /// The charge, or `None` on a machine with no battery — which is not a
    /// charge of nought, and a widget that drew it as one would colour a
    /// desktop as flat.
    pub fn charge(&self) -> Option<Percent> {
        self.context
            .read()
            .get::<BatteryState>()
            .map(|battery| Percent::of(battery.level))
    }

    /// How long is left, whichever direction it is going.
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
    /// # fn example(power: &omega::composite::Power) {
    /// if power.status() == omega::composite::Status::Unknown {
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
