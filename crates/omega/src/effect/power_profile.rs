//! Trading performance against power.

use omega_proto::omega::{PowerProfile, SetPowerProfile, action};

use crate::context::Context;
use crate::effect::does;

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
    /// Ask for a profile. The daemon may refuse it, or degrade it a moment
    /// later for thermals — either way the answer arrives as a reading rather
    /// than as a return value.
    pub fn set(&self, profile: PowerProfile) {
        self.act(action::Kind::SetPowerProfile(SetPowerProfile {
            profile: profile as i32,
        }));
    }
}
