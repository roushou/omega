//! Playback effects routed through the daemon.
use crate::{
    context::Context,
    effect::{Effect, does},
};
use omega_proto::{
    PlayerId,
    omega::{MediaKey, action, media_key},
};

/// Permission to control media playback from a command or reaction.
///
/// ```no_run
/// use omega::{Command, audio::{MediaControl, PlayerId}};
/// #[derive(omega::Command)]
/// struct Pause { media: MediaControl }
/// impl Command for Pause {
///     type Input = PlayerId;
///     type Output = ();
///     async fn call(&self, id: PlayerId) -> omega::Result<()> {
///         self.media.player(&id).pause().await
///     }
/// }
/// ```
///
/// ```compile_fail
/// #[derive(omega::Widget)]
/// struct Panel { media: omega::audio::MediaControl }
/// ```
#[derive(Debug)]
pub struct MediaControl {
    context: Context,
}
does!(MediaControl, Media);

impl MediaControl {
    /// Control the player selected by the daemon when the action executes.
    ///
    /// ```no_run
    /// # async fn example(media: &omega::audio::MediaControl) -> omega::Result<()> {
    /// media.active().play_pause().await
    /// # }
    /// ```
    pub fn active(&self) -> PlayerControl<'_> {
        PlayerControl {
            control: self,
            player: None,
        }
    }

    /// Control this endpoint. An unavailable endpoint never selects another player.
    /// The id may be reused by a restarted application.
    pub fn player(&self, id: &PlayerId) -> PlayerControl<'_> {
        PlayerControl {
            control: self,
            player: Some(id.clone()),
        }
    }
}

/// A media effect handle bound to an explicit or automatic target.
/// Obtained from [`MediaControl`]; merely choosing a target has no effect.
#[derive(Debug)]
pub struct PlayerControl<'a> {
    control: &'a MediaControl,
    player: Option<PlayerId>,
}

impl PlayerControl<'_> {
    /// Start or resume playback.
    pub fn play(&self) -> Effect {
        self.send(media_key::Key::MediaPlay)
    }

    /// Pause playback.
    pub fn pause(&self) -> Effect {
        self.send(media_key::Key::MediaPause)
    }

    /// Toggle playback at execution time.
    pub fn play_pause(&self) -> Effect {
        self.send(media_key::Key::MediaPlayPause)
    }

    /// Stop playback.
    pub fn stop(&self) -> Effect {
        self.send(media_key::Key::MediaStop)
    }

    /// Advance to the next track.
    pub fn next(&self) -> Effect {
        self.send(media_key::Key::MediaNext)
    }

    /// Return to the previous track.
    pub fn previous(&self) -> Effect {
        self.send(media_key::Key::MediaPrevious)
    }

    fn send(&self, key: media_key::Key) -> Effect {
        self.control.act(action::Kind::MediaKey(MediaKey {
            key: key as i32,
            player_id: self.player.as_ref().map(ToString::to_string),
        }))
    }
}
