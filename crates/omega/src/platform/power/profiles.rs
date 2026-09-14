//! Power profile state and control.

use omega_proto::omega::{SetPowerProfile, action};

use crate::runtime::context::Context;

use crate::wiring::does;

crate::wiring::reading! {
    /// Active and supported power profiles.
    PowerProfiles: omega_proto::omega::PowerProfileState
}

pub use omega_proto::omega::PowerProfile;

/// Display labels for power profiles.
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
    /// Return the active profile, or `None` if unavailable or unrecognized.
    pub fn active(&self) -> Option<PowerProfile> {
        let state = self.read()?;
        match PowerProfile::try_from(state.active) {
            Ok(PowerProfile::Unspecified) | Err(_) => None,
            Ok(profile) => Some(profile),
        }
    }

    /// Return supported profiles in the order reported by the service.
    /// Use this list to populate a profile selector.
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

    /// Performance limitation reported by the service, such as `lap-detected`,
    /// or `None` when no limitation is reported.
    pub fn degraded(&self) -> Option<String> {
        self.read()
            .map(|state| state.degraded)
            .filter(|reason| !reason.is_empty())
    }
}

/// Control the power profile. Use [`PowerProfiles`] to observe the active
/// profile and subsequent changes.
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
