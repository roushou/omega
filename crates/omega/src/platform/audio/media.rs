//! Media player metadata and playback control.

use crate::units::Remaining;

use crate::{effect::Effect, runtime::context::Context, wiring::does};

use omega_proto::{
    PlayerId,
    omega::{MediaKey, action, media_key},
};

crate::wiring::reading! {
    /// Current media players and playback state.
    Media: omega_proto::omega::MediaState
}

pub use omega_proto::omega::Playback;

/// One player, as MPRIS reports it.
#[derive(Debug, Clone, PartialEq)]
pub struct Player {
    id: omega_proto::PlayerId,
    identity: String,
    playback: Playback,
    title: String,
    artist: String,
    album: String,
    length: Option<Remaining>,
    active: bool,
    can_control: bool,
    can_play: bool,
    can_pause: bool,
    can_go_next: bool,
    can_go_previous: bool,
}

impl TryFrom<omega_proto::omega::PlayerInfo> for Player {
    type Error = omega_proto::PlayerIdError;

    fn try_from(player: omega_proto::omega::PlayerInfo) -> Result<Self, Self::Error> {
        Ok(Self {
            playback: Playback::try_from(player.playback).unwrap_or(Playback::Unspecified),
            // Convert MPRIS microseconds to the SDK duration type.
            length: match player.length_us {
                0 => None,
                us => Some(Remaining::of(std::time::Duration::from_micros(us))),
            },
            id: omega_proto::PlayerId::try_from(player.id)?,
            identity: player.identity,
            title: player.title,
            artist: player.artist,
            album: player.album,
            active: player.active,
            can_control: player.can_control,
            can_play: player.can_play,
            can_pause: player.can_pause,
            can_go_next: player.can_go_next,
            can_go_previous: player.can_go_previous,
        })
    }
}

impl Player {
    /// The bus name's suffix: `chromium`. A restarted app may reuse it.
    pub fn id(&self) -> &omega_proto::PlayerId {
        &self.id
    }

    /// Player display name, such as `Chromium`.
    pub fn identity(&self) -> &str {
        &self.identity
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    /// Artist names joined with a comma and space.
    pub fn artist(&self) -> &str {
        &self.artist
    }

    pub fn album(&self) -> &str {
        &self.album
    }

    pub fn playback(&self) -> Playback {
        self.playback
    }

    pub fn is_playing(&self) -> bool {
        self.playback == Playback::Playing
    }

    /// Whether this player accepts transport controls.
    pub fn can_control(&self) -> bool {
        self.can_control
    }
    /// Whether playback can be started.
    pub fn can_play(&self) -> bool {
        self.can_control && self.can_play
    }
    /// Whether playback can be paused.
    pub fn can_pause(&self) -> bool {
        self.can_control && self.can_pause
    }
    /// Whether the next track is available.
    pub fn can_go_next(&self) -> bool {
        self.can_control && self.can_go_next
    }
    /// Whether the previous track is available.
    pub fn can_go_previous(&self) -> bool {
        self.can_control && self.can_go_previous
    }

    /// Track duration, or `None` if the player does not report it.
    pub fn length(&self) -> Option<Remaining> {
        self.length
    }

    /// Whether the daemon has selected this player as active.
    pub fn is_active(&self) -> bool {
        self.active
    }
}

impl Media {
    /// Read the validated collection, distinguishing pending, unavailable, and
    /// malformed readings from a successfully empty collection.
    pub fn snapshot(&self) -> crate::platform::Reading<Vec<Player>> {
        self.context.read().media.clone()
    }

    /// Players in the current valid reading, or an empty collection otherwise.
    /// Use [`Self::snapshot`] to distinguish an empty reading from invalidity or absence.
    pub fn players(&self) -> Vec<Player> {
        self.snapshot().into_value().unwrap_or_default()
    }

    /// Return the active player, falling back to the first playing player.
    pub fn active(&self) -> Option<Player> {
        let players = self.players();
        players
            .iter()
            .find(|player| player.active)
            .or_else(|| players.iter().find(|player| player.is_playing()))
            .cloned()
    }
}

/// Permission to control media playback from a command or reaction.
///
/// ```no_run
/// use omega::{Command, platform::audio::{MediaControl, PlayerId}};
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
/// #[derive(omega::Surface)]
/// struct Panel { media: omega::platform::audio::MediaControl }
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
    /// # async fn example(media: &omega::platform::audio::MediaControl) -> omega::Result<()> {
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
