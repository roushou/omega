//! Audio output and media playback.

mod media;
mod output;
mod sinks;
mod streams;

pub use media::{Media, MediaControl, Playback, Player, PlayerControl};
pub use output::{Audio, SinkControl, SourceControl, Volume};
pub use sinks::{Sink, Sinks};
pub use streams::{Stream, StreamControl, Streams};

pub use omega_proto::{PlayerId, PlayerIdError};
