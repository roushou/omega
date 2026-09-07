//! Changing the volume. To *read* it, hold an `Audio`.

use omega_wire::omega::{SetVolume, action, set_volume};

use crate::context::Context;
use crate::effect::does;
use crate::units::Percent;

/// Permission to change the output volume.
#[derive(Debug)]
pub struct Volume {
    context: Context,
}

does!(Volume, Audio);

impl Volume {
    /// Set it outright.
    pub fn set(&self, level: Percent) {
        self.change(set_volume::Change::Absolute(level.fraction()));
    }

    /// Move it by a signed fraction: `0.05` is five percent louder.
    pub fn adjust(&self, delta: f64) {
        self.change(set_volume::Change::Delta(delta));
    }

    pub fn toggle_mute(&self) {
        self.change(set_volume::Change::ToggleMute(true));
    }

    fn change(&self, change: set_volume::Change) {
        self.act(action::Kind::SetVolume(SetVolume {
            change: Some(change),
        }));
    }
}
