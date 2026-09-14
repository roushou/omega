//! How the machine is trading performance against power.

use omega_proto::omega::{SetPowerProfile, action};

use crate::runtime::context::Context;

use crate::wiring::does;

crate::wiring::reading! {
    /// How it is trading performance against power.
    PowerProfiles: omega_proto::omega::PowerProfileState
}

pub use omega_proto::omega::PowerProfile;

/// What each profile is called where a person reads it.
///
/// On the enum rather than in every panel, so two units do not disagree about
/// whether it is "Saver" or "Power saver".
pub trait ProfileLabel {
    fn label(self) -> &'static str;
}

impl ProfileLabel for PowerProfile {
    fn label(self) -> &'static str {
        match self {
            Self::Saver => "Saver",
            Self::Balanced => "Balanced",
            Self::Performance => "Performance",
            Self::Unspecified => "Unknown",
        }
    }
}

impl PowerProfiles {
    /// The profile in force, or `None` on a machine with no profile daemon —
    /// and for one this build cannot name, which is a vendor profile Omega
    /// should not mislabel.
    pub fn active(&self) -> Option<PowerProfile> {
        let state = self.read()?;
        match PowerProfile::try_from(state.active) {
            Ok(PowerProfile::Unspecified) | Err(_) => None,
            Ok(profile) => Some(profile),
        }
    }

    /// What this machine offers, in the order the daemon lists them.
    ///
    /// A panel draws these rather than every profile the enum can name: a
    /// desktop without a performance mode should not get a button that fails.
    pub fn available(&self) -> Vec<PowerProfile> {
        self.read()
            .map(|state| {
                state
                    .available
                    .into_iter()
                    .filter_map(|value| PowerProfile::try_from(value).ok())
                    .filter(|profile| *profile != PowerProfile::Unspecified)
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Why performance is being held back, or `None` when it is not — the
    /// daemon's own reason, like `lap-detected`.
    pub fn degraded(&self) -> Option<String> {
        self.read()
            .map(|state| state.degraded)
            .filter(|reason| !reason.is_empty())
    }
}

/// Permission to change the machine's power profile.
///
/// The profile daemon owns the setting, so this asks rather than remembers:
/// what the machine settles on comes back as a reading on `power-profile`,
/// which is what redraws whatever was showing it. A unit that also wants to
/// *show* the profile holds `PowerProfiles` beside this.
#[derive(Debug)]
pub struct SetProfile {
    context: Context,
}

does!(SetProfile, SystemControl);

impl SetProfile {
    /// Ask for a profile. The receipt reports whether the request succeeded;
    /// later thermal degradation is reported through the profile reading.
    pub fn set(&self, profile: PowerProfile) -> crate::effect::Effect {
        self.act(action::Kind::SetPowerProfile(SetPowerProfile {
            profile: profile as i32,
        }))
    }
}
