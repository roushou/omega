//! The displays, from Hyprland.
//!
//! Hyprland speaks over two Unix sockets, not D-Bus: `.socket.sock` answers
//! one request per connection, and `.socket2.sock` streams events for as long
//! as it is held. So a reading is a fresh request and a wake-up is a line on
//! the long-lived stream.
//!
//! Purely signal-driven, with no poll under it. Monitors are plugged in and
//! unplugged, and the compositor says so; nothing about a display changes
//! quietly the way a signal strength does.

use std::path::PathBuf;

use async_trait::async_trait;
use serde::Deserialize;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

use omega_proto::SystemTopic;
use omega_proto::omega::{DisplayState, MonitorInfo, StatePatch, StateTopic, state_topic};

use crate::broker::{Broker, BrokerError};

/// One monitor, as `hyprctl -j monitors` describes it.
///
/// Only the fields the ontology carries. Hyprland reports two dozen more —
/// make, model, transform, VRR — and naming them here would be this broker
/// holding an opinion about a schema it does not own.
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Monitor {
    pub name: String,
    pub width: u32,
    pub height: u32,
    /// Hertz, with decimals: 60.026 is a real refresh rate, not a rounding.
    pub refresh_rate: f64,
    pub x: i32,
    pub y: i32,
    pub scale: f32,
    pub focused: bool,
    /// Hyprland's word for a monitor it is not driving.
    #[serde(default)]
    pub disabled: bool,
}

impl Monitor {
    /// The ontology's view of it.
    pub fn info(&self) -> MonitorInfo {
        MonitorInfo {
            id: self.name.clone(),
            connected: !self.disabled,
            width: self.width,
            height: self.height,
            // The schema carries millihertz because 60.026 Hz is not 60, and
            // a bar that reports the mode has to be able to say which.
            refresh_mhz: (self.refresh_rate * 1000.0).round().max(0.0) as u32,
            x: self.x,
            y: self.y,
            scale: self.scale,
            // Hyprland has no notion of a primary monitor; the focused one is
            // the nearest true thing, and it is what a bar means by it.
            primary: self.focused,
        }
    }
}

/// What `hyprctl -j monitors` answers, turned into the ontology.
///
/// Pure, and separate from asking: the answer is JSON, which a test can hold
/// without a compositor in the room.
#[derive(Debug)]
pub struct Monitors;

impl Monitors {
    pub fn parse(json: &str) -> Result<DisplayState, BrokerError> {
        let monitors: Vec<Monitor> =
            serde_json::from_str(json).map_err(|error| BrokerError::Unreadable {
                subsystem: "hyprland",
                detail: error.to_string(),
            })?;
        Ok(DisplayState {
            monitors: monitors.iter().map(Monitor::info).collect(),
        })
    }
}

/// Where Hyprland listens, and the event stream held open to it.
struct Link {
    dir: PathBuf,
    events: BufReader<UnixStream>,
}

impl Link {
    /// The events that change what a display looks like. Hyprland streams
    /// every window focus and workspace switch down the same socket, and
    /// re-reading the monitors on each of those would be a request per
    /// keystroke.
    const WATCHED: &'static [&'static str] = &[
        "monitoradded",
        "monitoraddedv2",
        "monitorremoved",
        "monitorremovedv2",
        "focusedmon",
        "focusedmonv2",
        "configreloaded",
    ];

    fn dir() -> Result<PathBuf, BrokerError> {
        let runtime =
            std::env::var("XDG_RUNTIME_DIR").map_err(|_| Self::missing("XDG_RUNTIME_DIR"))?;
        let signature = std::env::var("HYPRLAND_INSTANCE_SIGNATURE")
            .map_err(|_| Self::missing("HYPRLAND_INSTANCE_SIGNATURE"))?;
        Ok(PathBuf::from(runtime).join("hypr").join(signature))
    }

    async fn open() -> Result<Self, BrokerError> {
        let dir = Self::dir()?;
        let events = UnixStream::connect(dir.join(".socket2.sock"))
            .await
            .map_err(Self::unreadable)?;
        Ok(Self {
            dir,
            events: BufReader::new(events),
        })
    }

    /// Ask for the monitors. A fresh connection each time because Hyprland
    /// answers one request and closes.
    async fn read(&self) -> Result<DisplayState, BrokerError> {
        let mut socket = UnixStream::connect(self.dir.join(".socket.sock"))
            .await
            .map_err(Self::unreadable)?;
        socket
            .write_all(b"j/monitors")
            .await
            .map_err(Self::unreadable)?;
        socket.shutdown().await.map_err(Self::unreadable)?;

        let mut answer = String::new();
        socket
            .read_to_string(&mut answer)
            .await
            .map_err(Self::unreadable)?;
        Monitors::parse(&answer)
    }

    /// Wait for an event that changes the displays, ignoring the rest.
    ///
    /// `read_line` is cancel-safe only in the sense that it may lose a partial
    /// line, which for a stream of complete events means losing one wake-up —
    /// and the next event re-reads everything anyway, because a reading is the
    /// whole set of monitors rather than a delta.
    async fn wait(&mut self) -> Result<(), BrokerError> {
        loop {
            let mut line = String::new();
            match self.events.read_line(&mut line).await {
                Ok(0) => {
                    return Err(BrokerError::Unreadable {
                        subsystem: "hyprland",
                        detail: "the compositor closed the event socket".into(),
                    });
                }
                Ok(_) => {
                    let name = line.split(">>").next().unwrap_or("").trim();
                    if Self::WATCHED.contains(&name) {
                        return Ok(());
                    }
                }
                Err(error) => return Err(Self::unreadable(error)),
            }
        }
    }

    fn missing(variable: &'static str) -> BrokerError {
        BrokerError::Unreadable {
            subsystem: "hyprland",
            detail: format!("{variable} is not set; this is not a Hyprland session"),
        }
    }

    fn unreadable(error: impl std::fmt::Display) -> BrokerError {
        BrokerError::Unreadable {
            subsystem: "hyprland",
            detail: error.to_string(),
        }
    }
}

impl std::fmt::Debug for Link {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Link")
    }
}

#[derive(Debug, Default)]
pub struct Hyprland {
    link: Option<Link>,
    /// Set only after a reading succeeds, so a `next` cancelled mid-read asks
    /// again rather than waiting on an event whose state it already missed.
    primed: bool,
}

impl Hyprland {
    pub fn new() -> Self {
        Self::default()
    }

    fn patch(state: DisplayState) -> StatePatch {
        StatePatch {
            topics: vec![StateTopic {
                topic: SystemTopic::Display.as_str().into(),
                revision: 0, // the Hub assigns the real revision
                value: Some(state_topic::Value::Display(state)),
            }],
        }
    }
}

#[async_trait]
impl Broker for Hyprland {
    fn name(&self) -> &'static str {
        "hyprland"
    }

    fn topics(&self) -> &'static [SystemTopic] {
        &[SystemTopic::Display]
    }

    async fn next(&mut self) -> Result<StatePatch, BrokerError> {
        if self.link.is_none() {
            self.link = Some(Link::open().await?);
            self.primed = false;
        }

        if self.primed {
            let link = self.link.as_mut().expect("opened above");
            if let Err(error) = link.wait().await {
                self.link = None;
                self.primed = false;
                return Err(error);
            }
        }

        let state = self.link.as_ref().expect("opened above").read().await?;
        self.primed = true;
        Ok(Self::patch(state))
    }
}
