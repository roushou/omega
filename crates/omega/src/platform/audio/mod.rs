//! Audio output and media playback.

mod media;
mod output;
mod streams;

pub use media::{Media, MediaControl, Playback, Player, PlayerControl};
pub use output::{Audio, SinkControl, SourceControl, Volume};
pub use streams::{Stream, StreamControl, Streams};

pub use omega_proto::{PlayerId, PlayerIdError};
