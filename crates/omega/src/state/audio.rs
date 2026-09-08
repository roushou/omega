//! Sound output.

use crate::state::Audio;
use crate::units::Percent;

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
