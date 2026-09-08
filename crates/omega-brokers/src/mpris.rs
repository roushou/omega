//! What is playing, from MPRIS.
//!
//! Every player that exposes `org.mpris.MediaPlayer2` on the session bus, at
//! once: a browser tab and a music app is the normal case, not the awkward
//! one. Which of them a media key reaches is decided here rather than by
//! every widget that draws them.
//!
//! Woken by signals and polled underneath. Players emit `PropertiesChanged`
//! when what they are playing changes, but a player *starting* is a name
//! appearing on the bus — so a slow tick is the floor that notices one.

use std::collections::HashMap;
use std::time::Duration;

use async_trait::async_trait;
use futures_util::StreamExt;
use zbus::fdo::PropertiesProxy;
use zbus::names::InterfaceName;
use zbus::zvariant::OwnedValue;
use zbus::{Connection, MatchRule, MessageStream, Proxy};

use omega_proto::omega::{
    MediaKey, MediaState, Playback, PlayerInfo, StatePatch, StateTopic, action, media_key,
    state_topic,
};
use omega_proto::{ActionKind, SystemTopic};

use crate::broker::{Broker, BrokerError, Cadence};

/// One player, as MPRIS describes it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Player {
    /// The bus name's suffix, which is as close to an id as MPRIS has.
    pub id: String,
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

    fn info(&self, active: bool) -> PlayerInfo {
        PlayerInfo {
            id: self.id.clone(),
            identity: self.identity.clone(),
            playback: self.playback() as i32,
            title: self.title.clone(),
            // Several artists is one line to a bar. Joining here is what stops
            // every widget picking its own separator.
            artist: self.artists.join(", "),
            album: self.album.clone(),
            length_us: u64::try_from(self.length_us).unwrap_or(0),
            active,
        }
    }
}

/// The players, and which one a media key reaches.
#[derive(Debug)]
pub struct Players;

impl Players {
    /// The ontology's view: playing first, then by name.
    ///
    /// Sorted because the bus answers in whatever order it holds names, and a
    /// bar drawing "now playing" would otherwise swap between two paused
    /// players every refresh.
    pub fn state(players: &[Player]) -> MediaState {
        let mut sorted: Vec<&Player> = players.iter().collect();
        sorted.sort_by(|a, b| {
            b.is_playing()
                .cmp(&a.is_playing())
                .then_with(|| a.id.cmp(&b.id))
        });

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
        let connection = Connection::session().await.map_err(Self::unreadable)?;

        // One rule for every player, rather than a subscription per player
        // that would have to be torn down and rebuilt as they come and go.
        let rule = MatchRule::builder()
            .msg_type(zbus::message::Type::Signal)
            .interface("org.freedesktop.DBus.Properties")
            .map_err(Self::unreadable)?
            .member("PropertiesChanged")
            .map_err(Self::unreadable)?
            .add_arg(Self::PLAYER_IFACE)
            .map_err(Self::unreadable)?
            .build();

        let changes = MessageStream::for_match_rule(rule, &connection, None)
            .await
            .map_err(Self::unreadable)?;

        Ok(Self {
            connection,
            changes,
        })
    }

    /// Every MPRIS player on the bus.
    async fn read(&self) -> Result<Vec<Player>, BrokerError> {
        let bus = zbus::fdo::DBusProxy::new(&self.connection)
            .await
            .map_err(Self::unreadable)?;
        let names = bus.list_names().await.map_err(Self::unreadable)?;

        let mut players = Vec::new();
        for name in names {
            let Some(id) = name.as_str().strip_prefix(Self::PREFIX) else {
                continue;
            };
            if let Some(player) = self.player(name.as_str(), id).await {
                players.push(player);
            }
        }
        Ok(players)
    }

    /// One player, or nothing where it stopped answering mid-read — which is
    /// ordinary: a player quitting is exactly when this runs.
    async fn player(&self, bus_name: &str, id: &str) -> Option<Player> {
        let properties = PropertiesProxy::builder(&self.connection)
            .destination(bus_name.to_string())
            .ok()?
            .path(Self::PATH)
            .ok()?
            .build()
            .await
            .ok()?;

        let root = properties
            .get_all(InterfaceName::try_from(Self::ROOT_IFACE).ok()?)
            .await
            .ok()?;
        let player = properties
            .get_all(InterfaceName::try_from(Self::PLAYER_IFACE).ok()?)
            .await
            .ok()?;
        let metadata: HashMap<String, OwnedValue> = Self::get(&player, "Metadata")?;

        Some(Player {
            id: id.to_string(),
            identity: Self::get(&root, "Identity").unwrap_or_default(),
            status: Self::get(&player, "PlaybackStatus").unwrap_or_default(),
            title: Self::get(&metadata, "xesam:title").unwrap_or_default(),
            artists: Self::get(&metadata, "xesam:artist").unwrap_or_default(),
            album: Self::get(&metadata, "xesam:album").unwrap_or_default(),
            length_us: Self::get(&metadata, "mpris:length").unwrap_or(0),
        })
    }

    /// Ask one player to do something.
    async fn call(&self, bus_name: &str, method: &str) -> Result<(), BrokerError> {
        let player = Proxy::new(
            &self.connection,
            bus_name.to_string(),
            Self::PATH,
            Self::PLAYER_IFACE,
        )
        .await
        .map_err(Self::unreadable)?;
        player
            .call_method(method, &())
            .await
            .map(|_| ())
            .map_err(Self::unreadable)
    }

    fn get<T>(properties: &HashMap<String, OwnedValue>, name: &str) -> Option<T>
    where
        T: TryFrom<OwnedValue>,
    {
        T::try_from(properties.get(name)?.try_clone().ok()?).ok()
    }

    fn unreadable(error: impl std::fmt::Display) -> BrokerError {
        BrokerError::Unreadable {
            subsystem: "mpris",
            detail: error.to_string(),
        }
    }
}

impl std::fmt::Debug for Link {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Link")
    }
}

#[derive(Debug)]
pub struct Mpris {
    link: Option<Link>,
    tick: Cadence,
    primed: bool,
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
            primed: false,
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

    async fn next(&mut self) -> Result<StatePatch, BrokerError> {
        if self.link.is_none() {
            self.link = Some(Link::open().await?);
            self.primed = false;
        }

        if self.primed {
            let closed = {
                let Self { link, tick, .. } = self;
                let link = link.as_mut().expect("opened above");
                tokio::select! {
                    change = link.changes.next() => change.is_none(),
                    _ = tick.wait() => false,
                }
            };
            if closed {
                self.link = None;
                self.primed = false;
                return Err(BrokerError::Unreadable {
                    subsystem: "mpris",
                    detail: "the bus closed".into(),
                });
            }
        }

        let players = self.link.as_ref().expect("opened above").read().await?;
        self.primed = true;
        Ok(Self::patch(Players::state(&players)))
    }

    async fn act(&mut self, action: &action::Kind) -> Result<Option<StatePatch>, BrokerError> {
        let action::Kind::MediaKey(key) = action else {
            return Err(BrokerError::Unserved(ActionKind::of(action)));
        };
        let Some(method) = Players::method(key) else {
            return Err(BrokerError::Unreadable {
                subsystem: "mpris",
                detail: "MediaKey names no key".into(),
            });
        };

        if self.link.is_none() {
            self.link = Some(Link::open().await?);
            self.primed = false;
        }
        let link = self.link.as_ref().expect("opened above");

        let players = link.read().await?;
        let Some(target) = Players::active(&players) else {
            return Err(BrokerError::Unreadable {
                subsystem: "mpris",
                detail: "nothing is playing".into(),
            });
        };
        link.call(&target, method).await?;

        // The player answers with a PropertiesChanged of its own, which the
        // subscription is already waiting on.
        Ok(None)
    }
}
