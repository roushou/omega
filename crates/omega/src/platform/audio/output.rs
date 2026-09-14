//! Sound output.

use crate::units::Percent;

use omega_proto::omega::{SetVolume, action, set_volume};

use crate::runtime::context::Context;

use crate::wiring::does;

crate::wiring::reading! {
    /// The output volume.
    Audio: omega_proto::omega::AudioState
}

/// What the speakers are doing. To *change* them, hold a `Volume`.
impl Audio {
    /// The output level. Prints itself as `40%`.
    pub fn volume(&self) -> Percent {
        self.read()
            .map(|audio| Percent::of(audio.volume))
            .unwrap_or(Percent::ZERO)
    }

    pub fn is_muted(&self) -> bool {
        self.read().is_some_and(|audio| audio.muted)
    }
}

/// Permission to change the output volume.
#[derive(Debug)]
pub struct Volume {
    context: Context,
}

does!(Volume, Audio);

impl Volume {
    /// Set it outright.
    pub fn set(&self, level: Percent) -> crate::effect::Effect {
        self.change(set_volume::Change::Absolute(level.fraction()))
    }

    /// Move it by a signed fraction: `0.05` is five percent louder.
    pub fn adjust(&self, delta: f64) -> crate::effect::Effect {
        self.change(set_volume::Change::Delta(delta))
    }

    pub fn toggle_mute(&self) -> crate::effect::Effect {
        self.change(set_volume::Change::ToggleMute(true))
    }

    fn change(&self, change: set_volume::Change) -> crate::effect::Effect {
        self.act(action::Kind::SetVolume(SetVolume {
            change: Some(change),
        }))
    }
}
