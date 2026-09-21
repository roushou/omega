//! Per-application audio streams and their controls.

use omega_proto::omega::{AudioStream, SetStreamMute, SetStreamVolume, action};

use crate::runtime::context::Context;
use crate::units::Percent;

use crate::wiring::does;

crate::wiring::reading! {
    /// Per-application audio streams. Use [`StreamControl`] to change one.
    Streams: omega_proto::omega::AudioStreamsState
}

/// One application stream, as PulseAudio reports it.
#[derive(Debug, Clone, PartialEq)]
pub struct Stream {
    index: u32,
    app: String,
    volume: Percent,
    muted: bool,
}

impl From<AudioStream> for Stream {
    fn from(stream: AudioStream) -> Self {
        Self {
            index: stream.index,
            app: stream.app,
            volume: Percent::of(stream.volume),
            muted: stream.muted,
        }
    }
}

impl Stream {
    /// The pactl sink-input index, stable for the stream's lifetime.
    pub fn index(&self) -> u32 {
        self.index
    }

    /// The application's name, as the server knows it.
    pub fn app(&self) -> &str {
        &self.app
    }

    pub fn volume(&self) -> Percent {
        self.volume
    }

    pub fn is_muted(&self) -> bool {
        self.muted
    }
}

impl Streams {
    /// The current streams, or an empty collection when unavailable.
    pub fn streams(&self) -> Vec<Stream> {
        self.read()
            .map(|state| state.streams.into_iter().map(Stream::from).collect())
            .unwrap_or_default()
    }
}

/// Permission to control one application stream by its index.
///
/// ```no_run
/// use omega::{Command, Percent, platform::audio::{StreamControl, Streams}};
/// #[derive(omega::Command)]
/// struct Quiet { streams: Streams, control: StreamControl }
/// impl Command for Quiet {
///     const ID: &'static str = "quiet";
///
///     type Input = ();
///     type Output = ();
///     async fn call(&self, _: ()) -> omega::Result<()> {
///         for stream in self.streams.streams() {
///             self.control.set_muted(stream.index(), true).await?;
///         }
///         Ok(())
///     }
/// }
/// ```
#[derive(Debug)]
pub struct StreamControl {
    context: Context,
}

does!(StreamControl, Audio);

impl StreamControl {
    /// Set one stream's volume.
    pub fn set_volume(&self, index: u32, level: Percent) -> crate::effect::Effect {
        self.act(action::Kind::SetStreamVolume(SetStreamVolume {
            stream_index: index,
            absolute: level.fraction(),
        }))
    }

    /// Set one stream's mute state: `true` mutes and `false` unmutes.
    pub fn set_muted(&self, index: u32, muted: bool) -> crate::effect::Effect {
        self.act(action::Kind::SetStreamMute(SetStreamMute {
            stream_index: index,
            muted,
        }))
    }
}
