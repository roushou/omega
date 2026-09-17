//! Discover MPRIS players and publish metadata and playback state.
//! Property signals and fallback polling refresh players and detect new bus names.

use std::collections::HashMap;
use std::time::Duration;

use async_trait::async_trait;
use futures_util::StreamExt;
use zbus::fdo::PropertiesProxy;
use zbus::zvariant::OwnedValue;
use zbus::{Connection, MatchRule, MessageStream};

use omega_proto::omega::{
    MediaKey, MediaState, Playback, PlayerInfo, StatePatch, StateTopic, action, media_key,
    state_topic,
};
use omega_proto::{ActionKind, PlayerId, SystemTopic};

use crate::broker::{Broker, BrokerError, Cadence, opaque_debug};
use crate::dbus;

/// One player, as MPRIS describes it.
#[derive(Debug, Clone, PartialEq)]
pub struct Player {
    /// The bus name's suffix, which is as close to an id as MPRIS has.
    pub id: PlayerId,
    pub identity: String,
    /// `PlaybackStatus`: `Playing`, `Paused` or `Stopped`.
    pub status: String,
    pub title: String,
    /// `xesam:artist` is a list — a track can have several.
    pub artists: Vec<String>,
    pub album: String,
    /// `mpris:length`, microseconds. Signed on the wire and occasionally
    /// negative from a player that does not know yet.
    pub length_us: i64,
    pub can_control: bool,
    pub can_play: bool,
    pub can_pause: bool,
    pub can_go_next: bool,
    pub can_go_previous: bool,
}

impl Player {
    fn playback(&self) -> Playback {
        match self.status.as_str() {
            "Playing" => Playback::Playing,
            "Paused" => Playback::Paused,
            "Stopped" => Playback::Stopped,
            _ => Playback::Unspecified,
        }
    }

    fn is_playing(&self) -> bool {
        self.playback() == Playback::Playing
    }

    fn priority(&self, other: &Self) -> std::cmp::Ordering {
        other
            .is_playing()
            .cmp(&self.is_playing())
            .then_with(|| self.id.cmp(&other.id))
    }

    fn supports(&self, key: media_key::Key) -> bool {
        self.can_control
            && match key {
                media_key::Key::MediaPlay => self.can_play,
                media_key::Key::MediaPause => self.can_pause,
                media_key::Key::MediaPlayPause => {
                    if self.is_playing() {
                        self.can_pause
                    } else {
                        self.can_play
                    }
                }
                media_key::Key::MediaNext => self.can_go_next,
                media_key::Key::MediaPrevious => self.can_go_previous,
                media_key::Key::MediaStop => true,
                media_key::Key::MediaKeyUnspecified => false,
            }
    }

    fn info(&self, active: bool) -> PlayerInfo {
        PlayerInfo {
            id: self.id.to_string(),
            identity: self.identity.clone(),
            playback: self.playback() as i32,
            title: self.title.clone(),
            // Several artists is one line to a bar. Joining here is what stops
            // every widget picking its own separator.
            artist: self.artists.join(", "),
            album: self.album.clone(),
            length_us: u64::try_from(self.length_us).unwrap_or(0),
            active,
            can_control: self.can_control,
            can_play: self.can_play,
            can_pause: self.can_pause,
            can_go_next: self.can_go_next,
            can_go_previous: self.can_go_previous,
        }
    }
}

/// The players, and which one a media key reaches.
#[derive(Debug)]
pub struct Players;

impl Players {
    /// Sort playing players first, then by name for stable selection.
    pub fn state(players: &[Player]) -> MediaState {
        let mut sorted: Vec<&Player> = players.iter().collect();
        sorted.sort_by(|a, b| a.priority(b));

        MediaState {
            players: sorted
                .iter()
                .enumerate()
                // Sorted playing-first, so the first is the one a key
                // reaches: whichever is playing, or the first there is.
                .map(|(index, player)| player.info(index == 0))
                .collect(),
        }
    }

    /// The bus name of the player a media key reaches, if there is one.
    pub fn active(players: &[Player]) -> Option<String> {
        let state = Self::state(players);
        let id = state.players.into_iter().find(|player| player.active)?.id;
        Some(format!("{}{id}", Link::PREFIX))
    }

    /// Resolve an action without redirecting an explicit player selection.
    pub fn target<'a>(players: &'a [Player], key: &MediaKey) -> Result<&'a Player, BrokerError> {
        let player = match &key.player_id {
            Some(id) => {
                let id = PlayerId::try_from(id.clone()).map_err(BrokerError::unreadable)?;
                players.iter().find(|player| player.id == id)
            }
            None => players.iter().min_by(|a, b| a.priority(b)),
        }
        .ok_or_else(|| BrokerError::Unreadable("media player is no longer available".into()))?;
        let operation = media_key::Key::try_from(key.key).map_err(BrokerError::unreadable)?;
        if !player.supports(operation) {
            return Err(BrokerError::Unsupported(format!(
                "player {} cannot perform {}",
                player.id,
                Self::method(key).unwrap_or("unspecified operation")
            )));
        }
        Ok(player)
    }

    /// The MPRIS method for a key. `Stop` is a method; the rest are too.
    pub fn method(key: &MediaKey) -> Option<&'static str> {
        match media_key::Key::try_from(key.key).ok()? {
            media_key::Key::MediaPlay => Some("Play"),
            media_key::Key::MediaPause => Some("Pause"),
            media_key::Key::MediaPlayPause => Some("PlayPause"),
            media_key::Key::MediaNext => Some("Next"),
            media_key::Key::MediaPrevious => Some("Previous"),
            media_key::Key::MediaStop => Some("Stop"),
            media_key::Key::MediaKeyUnspecified => None,
        }
    }
}

#[zbus::proxy(
    interface = "org.mpris.MediaPlayer2.Player",
    default_path = "/org/mpris/MediaPlayer2"
)]
trait Transport {
    fn play(&self) -> zbus::Result<()>;
    fn pause(&self) -> zbus::Result<()>;
    fn play_pause(&self) -> zbus::Result<()>;
    fn next(&self) -> zbus::Result<()>;
    fn previous(&self) -> zbus::Result<()>;
    fn stop(&self) -> zbus::Result<()>;
}

/// The session bus, and the subscription to every player on it.
struct Link {
    connection: Connection,
    changes: MessageStream,
}

impl Link {
    const PREFIX: &'static str = "org.mpris.MediaPlayer2.";
    const PATH: &'static str = "/org/mpris/MediaPlayer2";
    const ROOT_IFACE: &'static str = "org.mpris.MediaPlayer2";
    const PLAYER_IFACE: &'static str = "org.mpris.MediaPlayer2.Player";

    async fn open() -> Result<Self, BrokerError> {
        let connection = Connection::session()
            .await
            .map_err(BrokerError::unreadable)?;

        // Use one bus match rule covering all MPRIS players.
        let rule = MatchRule::builder()
            .msg_type(zbus::message::Type::Signal)
            .interface("org.freedesktop.DBus.Properties")
            .map_err(BrokerError::unreadable)?
            .member("PropertiesChanged")
            .map_err(BrokerError::unreadable)?
            .add_arg(Self::PLAYER_IFACE)
            .map_err(BrokerError::unreadable)?
            .build();

        let changes = MessageStream::for_match_rule(rule, &connection, None)
            .await
            .map_err(BrokerError::unreadable)?;

        Ok(Self {
            connection,
            changes,
        })
    }

    /// Every MPRIS player on the bus.
    async fn read(&self) -> Result<Vec<Player>, BrokerError> {
        let bus = zbus::fdo::DBusProxy::new(&self.connection)
            .await
            .map_err(BrokerError::unreadable)?;
        let names = bus.list_names().await.map_err(BrokerError::unreadable)?;

        let mut players = Vec::new();
        for name in names {
            let Some(id) = name.as_str().strip_prefix(Self::PREFIX) else {
                continue;
            };
            let id = PlayerId::try_from(id.to_string()).map_err(BrokerError::unreadable)?;
            if let Some(player) = self.player(name.as_str(), id).await {
                players.push(player);
            }
        }
        Ok(players)
    }

    /// One player, or nothing where it stopped answering mid-read — which is
    /// ordinary: a player quitting is exactly when this runs.
    async fn player(&self, bus_name: &str, id: PlayerId) -> Option<Player> {
        let properties = PropertiesProxy::builder(&self.connection)
            .destination(bus_name.to_string())
            .ok()?
            .path(Self::PATH)
            .ok()?
            .build()
            .await
            .ok()?;

        let root = dbus::properties(&properties, Self::ROOT_IFACE).await.ok()?;
        let player = dbus::properties(&properties, Self::PLAYER_IFACE)
            .await
            .ok()?;
        let metadata: HashMap<String, OwnedValue> = dbus::field(&player, "Metadata")?;

        Some(Player {
            id,
            identity: dbus::field(&root, "Identity").unwrap_or_default(),
            status: dbus::field(&player, "PlaybackStatus").unwrap_or_default(),
            title: dbus::field(&metadata, "xesam:title").unwrap_or_default(),
            artists: dbus::field(&metadata, "xesam:artist").unwrap_or_default(),
            album: dbus::field(&metadata, "xesam:album").unwrap_or_default(),
            length_us: dbus::field(&metadata, "mpris:length").unwrap_or(0),
            can_control: dbus::field(&player, "CanControl").unwrap_or(false),
            can_play: dbus::field(&player, "CanPlay").unwrap_or(false),
            can_pause: dbus::field(&player, "CanPause").unwrap_or(false),
            can_go_next: dbus::field(&player, "CanGoNext").unwrap_or(false),
            can_go_previous: dbus::field(&player, "CanGoPrevious").unwrap_or(false),
        })
    }

    /// Ask one player to do something.
    async fn call(&self, id: &PlayerId, key: media_key::Key) -> Result<(), BrokerError> {
        // Address the current owner directly so disappearance cannot auto-start an app.
        let bus = zbus::fdo::DBusProxy::new(&self.connection)
            .await
            .map_err(BrokerError::unreadable)?;
        let destination = format!("{}{id}", Self::PREFIX);
        let owner = bus
            .get_name_owner(
                destination
                    .as_str()
                    .try_into()
                    .map_err(BrokerError::unreadable)?,
            )
            .await
            .map_err(BrokerError::unreadable)?;
        let player = TransportProxy::builder(&self.connection)
            .destination(owner)
            .map_err(BrokerError::unreadable)?
            .build()
            .await
            .map_err(BrokerError::unreadable)?;
        let result = match key {
            media_key::Key::MediaPlay => player.play().await,
            media_key::Key::MediaPause => player.pause().await,
            media_key::Key::MediaPlayPause => player.play_pause().await,
            media_key::Key::MediaNext => player.next().await,
            media_key::Key::MediaPrevious => player.previous().await,
            media_key::Key::MediaStop => player.stop().await,
            media_key::Key::MediaKeyUnspecified => {
                return Err(BrokerError::Unreadable("MediaKey names no key".into()));
            }
        };
        result.map_err(BrokerError::unreadable)
    }
}

opaque_debug!(Link);

#[derive(Debug)]
pub struct Mpris {
    link: Option<Link>,
    tick: Cadence,
}

impl Default for Mpris {
    fn default() -> Self {
        Self::new()
    }
}

impl Mpris {
    /// The floor under the signals. A player *starting* is a name appearing
    /// on the bus, which no player signals.
    pub const REFRESH: Duration = Duration::from_secs(5);

    pub fn new() -> Self {
        Self {
            link: None,
            tick: Cadence::after(Self::REFRESH),
        }
    }

    fn patch(state: MediaState) -> StatePatch {
        StatePatch {
            topics: vec![StateTopic {
                topic: SystemTopic::Media.as_str().into(),
                revision: 0, // the Hub assigns the real revision
                value: Some(state_topic::Value::Media(state)),
            }],
        }
    }
}

#[async_trait]
impl Broker for Mpris {
    fn name(&self) -> &'static str {
        "mpris"
    }

    fn topics(&self) -> &'static [SystemTopic] {
        &[SystemTopic::Media]
    }

    fn actions(&self) -> &'static [ActionKind] {
        &[ActionKind::MediaKey]
    }

    fn disconnect(&mut self) {
        self.link = None;
    }

    async fn connect(&mut self) -> Result<(), BrokerError> {
        self.link = Some(Link::open().await?);
        Ok(())
    }

    async fn wake(&mut self) -> Result<(), BrokerError> {
        let Self { link, tick, .. } = self;
        let link = link.as_mut().ok_or_else(BrokerError::gone)?;
        // Both arms are cancel-safe: a signal stream is a receiver, and an
        // interval keeps its own deadline.
        tokio::select! {
            change = link.changes.next() => match change {
                Some(_) => Ok(()),
                None => Err(BrokerError::gone()),
            },
            _ = tick.wait() => Ok(()),
        }
    }

    async fn read(&mut self) -> Result<StatePatch, BrokerError> {
        let link = self.link.as_ref().ok_or_else(BrokerError::gone)?;
        Ok(Self::patch(Players::state(&link.read().await?)))
    }

    async fn act(&mut self, action: &action::Kind) -> Result<Option<StatePatch>, BrokerError> {
        let action::Kind::MediaKey(key) = action else {
            return Err(BrokerError::Unserved(ActionKind::of(action)));
        };
        let link = self.link.as_ref().ok_or_else(BrokerError::gone)?;
        let players = link.read().await?;
        let target = Players::target(&players, key)?;
        let operation = media_key::Key::try_from(key.key).map_err(BrokerError::unreadable)?;
        link.call(&target.id, operation).await?;

        // The player answers with a PropertiesChanged of its own, which the
        // subscription is already waiting on.
        Ok(None)
    }
}
