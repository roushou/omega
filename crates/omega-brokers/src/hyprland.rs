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

use omega_proto::omega::{
    Direction, DisplayState, MonitorInfo, StatePatch, StateTopic, WindowSelector, action,
    move_to_workspace, state_topic, switch_workspace, window_selector,
};
use omega_proto::{ActionKind, SystemTopic};

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

/// One action, as a Hyprland dispatcher.
///
/// Pure, and the whole of what this broker decides. Omega's words and
/// Hyprland's are not the same words — `Shutdown` was `PowerOff` for logind
/// and here `CloseWindow` on the focused window is `killactive` — so the
/// mapping is a function over an action rather than a walk through a socket,
/// and a test can read every line of it.
#[derive(Debug)]
pub struct Dispatch;

impl Dispatch {
    /// The dispatcher for an action, or `None` where Hyprland has no way to
    /// do what was asked. `None` is refused rather than sent: a dispatch
    /// Hyprland does not understand is answered "ok" by the socket, so
    /// guessing would report a window closed that is still open.
    pub fn of(action: &action::Kind) -> Option<String> {
        match action {
            action::Kind::SwitchWorkspace(switch) => Some(format!(
                "workspace {}",
                Self::workspace(switch.target.as_ref()?)
            )),
            action::Kind::MoveToWorkspace(move_to) => Some(format!(
                "movetoworkspace {},{}",
                Self::destination(move_to.target.as_ref()?),
                Self::window(move_to.window.as_ref())?
            )),
            action::Kind::MoveToMonitor(move_to) => Some(format!(
                "movewindow mon:{}",
                Self::named(&move_to.monitor_id)?
            )),
            action::Kind::CloseWindow(close) => {
                Some(match Self::is_focused(close.window.as_ref()) {
                    // The focused window has its own dispatcher, and it is the
                    // one that works when nothing matches a selector.
                    true => "killactive".to_string(),
                    false => format!("closewindow {}", Self::window(close.window.as_ref())?),
                })
            }
            action::Kind::ToggleFloating(toggle) => Some(format!(
                "togglefloating {}",
                Self::window(toggle.window.as_ref())?
            )),
            // Hyprland fullscreens the focused window and takes no selector.
            // Asking for another window is refused rather than silently done
            // to whichever one happens to be focused.
            action::Kind::ToggleFullscreen(toggle) => {
                Self::is_focused(toggle.window.as_ref()).then(|| "fullscreen 1".to_string())
            }
            _ => None,
        }
    }

    fn workspace(target: &switch_workspace::Target) -> String {
        match target {
            switch_workspace::Target::Index(index) => index.to_string(),
            switch_workspace::Target::Name(name) => format!("name:{name}"),
            // Hyprland's relative form, which wraps within the monitor.
            switch_workspace::Target::Direction(direction) => {
                match Direction::try_from(*direction) {
                    Ok(Direction::Previous) => "e-1".to_string(),
                    _ => "e+1".to_string(),
                }
            }
        }
    }

    fn destination(target: &move_to_workspace::Target) -> String {
        match target {
            move_to_workspace::Target::Index(index) => index.to_string(),
            move_to_workspace::Target::Name(name) => format!("name:{name}"),
        }
    }

    /// A window, as Hyprland selects one. An unset selector means the focused
    /// window, which is what every one of these dispatchers defaults to.
    fn window(selector: Option<&WindowSelector>) -> Option<String> {
        match selector.and_then(|selector| selector.target.as_ref()) {
            None | Some(window_selector::Target::Focused(_)) => Some("activewindow".to_string()),
            Some(window_selector::Target::AppId(id)) => Some(format!("class:{}", Self::named(id)?)),
            Some(window_selector::Target::Title(title)) => {
                Some(format!("title:{}", Self::named(title)?))
            }
        }
    }

    fn is_focused(selector: Option<&WindowSelector>) -> bool {
        matches!(
            selector.and_then(|selector| selector.target.as_ref()),
            None | Some(window_selector::Target::Focused(_))
        )
    }

    /// A name that could be pasted into a dispatch line.
    ///
    /// Empty is nothing to select by, and a newline would end the line and
    /// make the rest of it a second dispatch — so both are refused rather
    /// than sent.
    fn named(name: &str) -> Option<&str> {
        match name.is_empty() || name.contains(['\n', '\r', ';']) {
            true => None,
            false => Some(name),
        }
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

    /// Send a dispatch, and believe the answer.
    ///
    /// Hyprland answers `ok` or a sentence about what went wrong. Treating
    /// anything else as a failure is what stops a refused dispatch reading as
    /// a window that closed.
    async fn dispatch(&self, command: &str) -> Result<(), BrokerError> {
        let mut socket = UnixStream::connect(self.dir.join(".socket.sock"))
            .await
            .map_err(Self::unreadable)?;
        socket
            .write_all(format!("dispatch {command}").as_bytes())
            .await
            .map_err(Self::unreadable)?;
        socket.shutdown().await.map_err(Self::unreadable)?;

        let mut answer = String::new();
        socket
            .read_to_string(&mut answer)
            .await
            .map_err(Self::unreadable)?;

        match answer.trim() == "ok" {
            true => Ok(()),
            false => Err(BrokerError::Unreadable {
                subsystem: "hyprland",
                detail: format!("refused {command:?}: {}", answer.trim()),
            }),
        }
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

    fn actions(&self) -> &'static [ActionKind] {
        &[
            ActionKind::SwitchWorkspace,
            ActionKind::MoveToWorkspace,
            ActionKind::MoveToMonitor,
            ActionKind::CloseWindow,
            ActionKind::ToggleFloating,
            ActionKind::ToggleFullscreen,
        ]
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

    async fn act(&mut self, action: &action::Kind) -> Result<Option<StatePatch>, BrokerError> {
        let Some(command) = Dispatch::of(action) else {
            return Err(BrokerError::Unserved(ActionKind::of(action)));
        };

        if self.link.is_none() {
            self.link = Some(Link::open().await?);
            self.primed = false;
        }
        self.link
            .as_ref()
            .expect("opened above")
            .dispatch(&command)
            .await?;

        // Moving a window changes no display. What did change arrives on the
        // event socket if it is anything this broker reports.
        Ok(None)
    }
}
