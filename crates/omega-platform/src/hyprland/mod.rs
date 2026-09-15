//! Hyprland state and actions over its command and event sockets.
//! Queries use separate connections; a persistent event stream triggers refreshes.

use std::path::PathBuf;

use async_trait::async_trait;
use serde::Deserialize;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

use omega_proto::omega::{
    InputState, MonitorInfo, MonitorsState, StatePatch, StateTopic, WindowInfo, WindowState,
    WorkspaceInfo, WorkspacesState, action, state_topic,
};
use omega_proto::{ActionKind, SystemTopic};

use crate::broker::{Broker, BrokerError, opaque_debug};

mod dispatch;
pub use dispatch::{Dispatch, DispatchMode};

/// Monitor fields used by the protocol projection.
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
            // Hyprland has no primary output flag; project its focused output as primary.
            primary: self.focused,
        }
    }
}

/// Decode monitor query JSON into protocol state.
#[derive(Debug)]
pub struct Monitors;

impl Monitors {
    pub fn parse(json: &str) -> Result<MonitorsState, BrokerError> {
        let monitors: Vec<Monitor> = serde_json::from_str(json)
            .map_err(|error| BrokerError::Unreadable(error.to_string()))?;
        Ok(MonitorsState {
            monitors: monitors.iter().map(Monitor::info).collect(),
        })
    }
}

/// One workspace, as `hyprctl -j workspaces` describes it.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct Workspace {
    pub id: i32,
    pub name: String,
    /// The monitor's name, unlike the window's, which is a number.
    pub monitor: String,
    pub windows: u32,
}

/// The focused window, as `hyprctl -j activewindow` describes it.
///
/// Hyprland answers `{}` when nothing has focus, so every field defaults.
#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
pub struct Focused {
    #[serde(default)]
    pub class: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub workspace: FocusedWorkspace,
    #[serde(default)]
    pub pid: u32,
    #[serde(default)]
    pub floating: bool,
    /// A mode, not a flag: zero is not fullscreen.
    #[serde(default)]
    pub fullscreen: i32,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
pub struct FocusedWorkspace {
    #[serde(default)]
    pub name: String,
}

/// What the compositor answered, turned into the ontology.
#[derive(Debug)]
pub struct Session;

impl Session {
    pub fn workspaces(json: &str, active: Option<i32>) -> Result<WorkspacesState, BrokerError> {
        let mut found: Vec<Workspace> = Self::parse(json)?;
        // Sort workspaces by ID for stable output.
        found.sort_by_key(|workspace| workspace.id);

        Ok(WorkspacesState {
            workspaces: found
                .into_iter()
                .map(|workspace| WorkspaceInfo {
                    id: workspace.id,
                    active: Some(workspace.id) == active,
                    name: workspace.name,
                    monitor_id: workspace.monitor,
                    windows: workspace.windows,
                })
                .collect(),
        })
    }

    /// Decode the focused window; an empty object means no focused window.
    pub fn window(json: &str) -> Result<WindowState, BrokerError> {
        let focused: Focused = Self::parse(json)?;
        Ok(WindowState {
            focused: (!focused.class.is_empty() || !focused.title.is_empty()).then(|| WindowInfo {
                app_id: focused.class,
                title: focused.title,
                workspace: focused.workspace.name,
                // Hyprland reports a numeric monitor here and the ontology
                // carries names. The workspace above already says which.
                monitor_id: String::new(),
                pid: focused.pid,
                floating: focused.floating,
                fullscreen: focused.fullscreen != 0,
            }),
        })
    }

    /// Read focused workspace identity; an empty object means no active workspace.
    pub fn active_id(json: &str) -> Result<Option<i32>, BrokerError> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum ActiveWorkspace {
            Present { id: i32 },
            Absent(Empty),
        }
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Empty {}
        Ok(match Self::parse::<ActiveWorkspace>(json)? {
            ActiveWorkspace::Present { id } => Some(id),
            ActiveWorkspace::Absent(_) => None,
        })
    }

    fn parse<T: serde::de::DeserializeOwned>(json: &str) -> Result<T, BrokerError> {
        serde_json::from_str(json).map_err(|error| BrokerError::Unreadable(error.to_string()))
    }
}

/// What `hyprctl -j devices` reports, of which only the keyboards are read.
#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
pub struct Devices {
    #[serde(default)]
    pub keyboards: Vec<Keyboard>,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
pub struct Keyboard {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub layout: String,
    #[serde(default)]
    pub active_keymap: String,
    /// Whether Hyprland marks this keyboard as main.
    #[serde(default)]
    pub main: bool,
}

impl Session {
    /// Select the compositor's main keyboard and its active layout.
    pub fn input(json: &str) -> Result<InputState, BrokerError> {
        let devices: Devices = Self::parse(json)?;
        let main = devices
            .keyboards
            .iter()
            .find(|keyboard| keyboard.main)
            // Report absence when no main keyboard exists.
            .cloned()
            .unwrap_or_default();

        Ok(InputState {
            keyboard: main.name,
            layout: main.layout,
            keymap: main.active_keymap,
        })
    }
}

/// Where Hyprland listens, and the event stream held open to it.
struct Link {
    dir: PathBuf,
    events: tokio::io::Lines<BufReader<UnixStream>>,
}

impl Link {
    /// Map compositor events to the topics that need refreshing.
    fn affected(event: &str) -> &'static [SystemTopic] {
        const FOCUS: &[SystemTopic] = &[
            SystemTopic::Monitors,
            SystemTopic::Workspaces,
            SystemTopic::Window,
        ];
        const WORKSPACES: &[SystemTopic] = &[SystemTopic::Workspaces];
        const WINDOW: &[SystemTopic] = &[SystemTopic::Window];
        const INPUT: &[SystemTopic] = &[SystemTopic::Input];
        // Opening or closing a window changes what has focus *and* the count
        // on the workspace it was on.
        const BOTH: &[SystemTopic] = &[SystemTopic::Workspaces, SystemTopic::Window];
        // A monitor arriving moves workspaces onto it and takes focus with
        // them.
        const EVERYTHING: &[SystemTopic] = &[
            SystemTopic::Monitors,
            SystemTopic::Workspaces,
            SystemTopic::Window,
            SystemTopic::Input,
        ];

        match event {
            "monitoradded" | "monitoraddedv2" | "monitorremoved" | "monitorremovedv2"
            | "configreloaded" => EVERYTHING,
            "focusedmon" | "focusedmonv2" => FOCUS,
            "workspace" | "workspacev2" => BOTH,
            "createworkspace" | "createworkspacev2" | "destroyworkspace" | "destroyworkspacev2"
            | "moveworkspace" | "moveworkspacev2" | "renameworkspace" => WORKSPACES,
            "openwindow" | "closewindow" | "movewindow" | "movewindowv2" => BOTH,
            "activewindow" | "activewindowv2" | "windowtitle" | "windowtitlev2" | "fullscreen"
            | "changefloatingmode" => WINDOW,
            "activelayout" => INPUT,
            _ => &[],
        }
    }

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
            .map_err(BrokerError::unreadable)?;
        Ok(Self {
            dir,
            events: BufReader::new(events).lines(),
        })
    }

    /// Read exactly the topics named, and nothing else.
    async fn read(&self, wanted: &[SystemTopic]) -> Result<StatePatch, BrokerError> {
        let mut topics = Vec::with_capacity(wanted.len());

        for topic in wanted {
            let value = match topic {
                SystemTopic::Monitors => {
                    state_topic::Value::Monitors(Monitors::parse(&self.ask("j/monitors").await?)?)
                }
                SystemTopic::Workspaces => {
                    let active = Session::active_id(&self.ask("j/activeworkspace").await?)?;
                    state_topic::Value::Workspaces(Session::workspaces(
                        &self.ask("j/workspaces").await?,
                        active,
                    )?)
                }
                SystemTopic::Window => {
                    state_topic::Value::Window(Session::window(&self.ask("j/activewindow").await?)?)
                }
                SystemTopic::Input => {
                    state_topic::Value::Input(Session::input(&self.ask("j/devices").await?)?)
                }
                // Nothing else is this broker's, and the driver only ever
                // asks for what `affected` named.
                _ => continue,
            };
            topics.push(StateTopic {
                topic: topic.as_str().into(),
                revision: 0, // the Hub assigns the real revision
                value: Some(value),
            });
        }

        Ok(StatePatch { topics })
    }

    /// One request. A fresh connection each time, because Hyprland answers
    /// one and closes.
    async fn ask(&self, request: &str) -> Result<String, BrokerError> {
        let mut socket = UnixStream::connect(self.dir.join(".socket.sock"))
            .await
            .map_err(BrokerError::unreadable)?;
        socket
            .write_all(request.as_bytes())
            .await
            .map_err(BrokerError::unreadable)?;
        socket.shutdown().await.map_err(BrokerError::unreadable)?;

        let mut answer = String::new();
        socket
            .read_to_string(&mut answer)
            .await
            .map_err(BrokerError::unreadable)?;
        Ok(answer)
    }

    /// Require the compositor's `ok` response; other replies are dispatch failures.
    async fn dispatch(&self, command: &str) -> Result<(), BrokerError> {
        let mut socket = UnixStream::connect(self.dir.join(".socket.sock"))
            .await
            .map_err(BrokerError::unreadable)?;
        socket
            .write_all(format!("dispatch {command}").as_bytes())
            .await
            .map_err(BrokerError::unreadable)?;
        socket.shutdown().await.map_err(BrokerError::unreadable)?;

        let mut answer = String::new();
        socket
            .read_to_string(&mut answer)
            .await
            .map_err(BrokerError::unreadable)?;

        if answer.trim() == "ok" {
            Ok(())
        } else {
            Err(BrokerError::unreadable(format!(
                "refused {command:?}: {}",
                answer.trim()
            )))
        }
    }

    /// Wait for an event that changes the displays, ignoring the rest.
    ///
    /// The line decoder retains partial input when an action interrupts the wait.
    async fn wait(&mut self) -> Result<&'static [SystemTopic], BrokerError> {
        loop {
            match self.events.next_line().await {
                Ok(None) => {
                    return Err(BrokerError::Unreadable(
                        "the compositor closed the event socket".into(),
                    ));
                }
                Ok(Some(line)) => {
                    let name = line.split(">>").next().unwrap_or("").trim();
                    let affected = Self::affected(name);
                    if !affected.is_empty() {
                        return Ok(affected);
                    }
                }
                Err(error) => return Err(BrokerError::unreadable(error)),
            }
        }
    }

    fn missing(variable: &'static str) -> BrokerError {
        BrokerError::unreadable(format!(
            "{variable} is not set; this is not a Hyprland session"
        ))
    }
}

opaque_debug!(Link);

#[derive(Debug)]
pub struct Hyprland {
    link: Option<Link>,
    /// Retain topic invalidations between wake and read.
    /// A new connection requires all supported topics to be read.
    affected: &'static [SystemTopic],
}

impl Default for Hyprland {
    fn default() -> Self {
        Self {
            link: None,
            affected: Self::EVERYTHING,
        }
    }
}

impl Hyprland {
    /// Read every supported topic after connecting.
    const EVERYTHING: &'static [SystemTopic] = &[
        SystemTopic::Monitors,
        SystemTopic::Workspaces,
        SystemTopic::Window,
        SystemTopic::Input,
    ];

    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl Broker for Hyprland {
    fn name(&self) -> &'static str {
        "hyprland"
    }

    fn topics(&self) -> &'static [SystemTopic] {
        Self::EVERYTHING
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

    fn disconnect(&mut self) {
        self.link = None;
    }

    async fn connect(&mut self) -> Result<(), BrokerError> {
        self.link = Some(Link::open().await?);
        self.affected = Self::EVERYTHING;
        Ok(())
    }

    async fn wake(&mut self) -> Result<(), BrokerError> {
        let link = self.link.as_mut().ok_or_else(BrokerError::gone)?;
        self.affected = link.wait().await?;
        Ok(())
    }

    async fn read(&mut self) -> Result<StatePatch, BrokerError> {
        let wanted = self.affected;
        self.link
            .as_ref()
            .ok_or_else(BrokerError::gone)?
            .read(wanted)
            .await
    }

    async fn act(&mut self, action: &action::Kind) -> Result<Option<StatePatch>, BrokerError> {
        let link = self.link.as_ref().ok_or_else(BrokerError::gone)?;
        let mode = DispatchMode::from_status(&link.ask("j/status").await?)?;
        let Some(command) = Dispatch::for_mode(action, mode) else {
            return Err(BrokerError::Unserved(ActionKind::of(action)));
        };

        link.dispatch(&command).await?;

        // Acknowledgement is not observed state. The event stream drives readings.
        Ok(None)
    }
}

#[cfg(test)]
mod tests;
