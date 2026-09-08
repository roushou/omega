//! Sound output.

use omega_proto::omega::AudioState;

use crate::context::Context;
use crate::source::reads;
use crate::units::Percent;

/// What the speakers are doing. To *change* them, hold a `Volume`.
#[derive(Debug)]
pub struct Audio {
    context: Context,
}

reads!(Audio, AudioState);

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
