//! Audio output and media playback.

mod media;
mod output;

pub use media::{Media, MediaControl, Playback, Player, PlayerControl};
pub use output::{Audio, Volume};

pub use omega_proto::{PlayerId, PlayerIdError};
