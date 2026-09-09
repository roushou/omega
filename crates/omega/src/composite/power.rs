//! What the machine is doing about power.

use omega_proto::SystemTopic;
use omega_proto::omega::Capability;

use crate::context::Context;
use crate::reading::{Battery, Mains};
use crate::units::{Percent, Remaining};
use crate::wiring::{Reads, Wiring};

/// The machine's power situation, in one word.
///
/// An enum rather than a string because these are four states and not four
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
    /// No battery at all, and no cable reported either — a desktop before
    /// anything has been said about it.
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
    battery: Battery,
    mains: Mains,
}

impl Wiring for Power {
    const TOPICS: &'static [SystemTopic] = &[SystemTopic::Battery, SystemTopic::Mains];
    const CAPABILITIES: &'static [Capability] = &[Capability::StateRead];

    fn build(context: &Context) -> Self {
        Self {
            battery: Battery::build(context),
            mains: Mains::build(context),
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
        self.battery.has_reading()
    }

    /// Whether the cable is in.
    pub fn on_mains(&self) -> bool {
        self.mains.is_connected()
    }

    pub fn is_charging(&self) -> bool {
        self.battery.is_charging()
    }

    /// The charge, or `None` on a machine with no battery — which is not a
    /// charge of nought, and a widget that drew it as one would colour a
    /// desktop as flat.
    pub fn charge(&self) -> Option<Percent> {
        self.has_battery().then(|| self.battery.charge())
    }

    /// How long is left, whichever direction it is going.
    pub fn remaining(&self) -> Option<Remaining> {
        self.battery.remaining()
    }

    /// What the machine is doing about power.
    ///
    /// The one thing a composite is for. Charging and on-mains are different
    /// questions, and a full battery on the wall is answering no to the first
    /// and yes to the second — which is the case every hand-written version
    /// of this got wrong.
    pub fn status(&self) -> Status {
        let Some(charge) = self.charge() else {
            return match self.on_mains() {
                true => Status::OnMains,
                false => Status::Unknown,
            };
        };

        match (self.is_charging(), self.on_mains()) {
            (true, _) => Status::Charging,
            (false, true) if charge >= Self::FULL => Status::FullyCharged,
            (false, true) => Status::OnMains,
            (false, false) => Status::OnBattery,
        }
    }
}
