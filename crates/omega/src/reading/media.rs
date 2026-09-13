//! What is playing.

use crate::reading::Media;
use crate::units::Remaining;

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

impl Player {
    fn of(player: omega_proto::omega::PlayerInfo) -> Self {
        Self {
            playback: Playback::try_from(player.playback).unwrap_or(Playback::Unspecified),
            // Microseconds on the wire, because that is how MPRIS counts.
            // Nowhere above this should have to know that.
            length: match player.length_us {
                0 => None,
                us => Some(Remaining::of(std::time::Duration::from_micros(us))),
            },
            id: omega_proto::PlayerId::parse(player.id).expect("daemon supplied a valid player id"),
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
        }
    }

    /// The bus name's suffix: `chromium`. A restarted app may reuse it.
    pub fn id(&self) -> &omega_proto::PlayerId {
        &self.id
    }

    /// What the player calls itself: `Chromium`.
    pub fn identity(&self) -> &str {
        &self.identity
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    /// Every artist, joined.
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

    /// How long the track runs. Prints itself as `3m`.
    pub fn length(&self) -> Option<Remaining> {
        self.length
    }

    /// Whether this is the one the machine considers current.
    pub fn is_active(&self) -> bool {
        self.active
    }
}

impl Media {
    pub fn players(&self) -> Vec<Player> {
        self.read()
            .map(|state| state.players.into_iter().map(Player::of).collect())
            .unwrap_or_default()
    }

    /// The one a bar slot should show: whichever is marked active, else
    /// whichever is playing.
    pub fn active(&self) -> Option<Player> {
        let players = self.players();
        players
            .iter()
            .find(|player| player.active)
            .or_else(|| players.iter().find(|player| player.is_playing()))
            .cloned()
    }
}
