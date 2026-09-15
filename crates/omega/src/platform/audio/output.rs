//! Audio output state and volume control.

use crate::units::Percent;

use omega_proto::omega::{SetVolume, action, set_volume};

use crate::runtime::context::Context;

use crate::wiring::does;

crate::wiring::reading! {
    /// The output volume.
    Audio: omega_proto::omega::AudioState
}

/// Current audio output state. Use [`Volume`] to change it.
impl Audio {
    /// Output volume as a percentage.
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
    /// Set the output volume.
    pub fn set(&self, level: Percent) -> crate::effect::Effect {
        self.change(set_volume::Change::Absolute(level.fraction()))
    }

    /// Adjust volume by a signed fraction; `0.05` adds five percentage points.
    pub fn adjust(&self, delta: f64) -> crate::effect::Effect {
        self.change(set_volume::Change::Delta(delta))
    }

    pub fn toggle_mute(&self) -> crate::effect::Effect {
        self.change(set_volume::Change::ToggleMute(true))
    }

    /// Set the default output's mute state: `true` mutes and `false` unmutes.
    ///
    /// Sends an absolute state without consulting an audio reading. Repeating
    /// the same value does not toggle the output. The backend resolves the default
    /// output when executing the request; backend failures are returned by the effect.
    ///
    /// ```no_run
    /// # async fn example(volume: &omega::platform::audio::Volume) -> omega::Result<()> {
    /// volume.set_muted(true).await?;
    /// volume.set_muted(false).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn set_muted(&self, muted: bool) -> crate::effect::Effect {
        self.change(set_volume::Change::Muted(muted))
    }

    fn change(&self, change: set_volume::Change) -> crate::effect::Effect {
        self.act(action::Kind::SetVolume(SetVolume {
            change: Some(change),
        }))
    }
}
