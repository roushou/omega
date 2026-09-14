//! Read and control the default audio sink through pactl.
//! A persistent subscription triggers JSON queries after sink or server changes.

use async_trait::async_trait;
use serde::Deserialize;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};

use omega_proto::omega::{AudioState, StatePatch, StateTopic, action, set_volume, state_topic};
use omega_proto::{ActionKind, SystemTopic};

use crate::broker::{Broker, BrokerError, opaque_debug};

/// One sink, as `pactl --format=json list sinks` describes it.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct Sink {
    pub name: String,
    pub mute: bool,
    /// Per-channel volume values.
    pub volume: std::collections::HashMap<String, Channel>,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq)]
pub struct Channel {
    /// Raw PulseAudio volume, where 65536 is unattenuated.
    pub value: u32,
}

/// Decode pactl JSON and convert PulseAudio volume units.
#[derive(Debug)]
pub struct Sinks;

impl Sinks {
    /// `PA_VOLUME_NORM`: the raw value that means unattenuated.
    const NORM: f64 = 65536.0;

    pub fn parse(json: &str, default: &str) -> Result<AudioState, BrokerError> {
        let sinks: Vec<Sink> = serde_json::from_str(json)
            .map_err(|error| BrokerError::Unreadable(error.to_string()))?;
        Ok(Self::of(&sinks, default))
    }

    /// Read the default sink. If no sink exists, report zero volume and muted state.
    pub fn of(sinks: &[Sink], default: &str) -> AudioState {
        let Some(sink) = sinks.iter().find(|sink| sink.name == default) else {
            return AudioState {
                volume: 0.0,
                muted: true,
                default_sink: default.to_string(),
            };
        };

        AudioState {
            volume: Self::level(sink),
            muted: sink.mute,
            default_sink: sink.name.clone(),
        }
    }

    /// Return the loudest channel as a fraction, clamped to 1.0.
    fn level(sink: &Sink) -> f64 {
        sink.volume
            .values()
            .map(|channel| f64::from(channel.value) / Self::NORM)
            .fold(0.0, f64::max)
            .clamp(0.0, 1.0)
    }
}

/// `pactl`, and the subscription held open to it.
struct Link {
    events: tokio::io::Lines<BufReader<tokio::process::ChildStdout>>,
    /// Dropping the broker must terminate its subscription process.
    _child: Child,
}

impl Link {
    /// Refresh only for sink and server changes, not individual audio streams.
    const WATCHED: &'static [&'static str] = &["on sink", "on server"];

    async fn open() -> Result<Self, BrokerError> {
        let mut child = Command::new("pactl")
            .arg("subscribe")
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(BrokerError::unreadable)?;

        let stdout = child.stdout.take().ok_or(BrokerError::Unreadable(
            "pactl gave no output to read".into(),
        ))?;

        Ok(Self {
            events: BufReader::new(stdout).lines(),
            _child: child,
        })
    }

    /// Wait for a default-output state change.
    async fn wait(&mut self) -> Result<(), BrokerError> {
        loop {
            match self.events.next_line().await {
                Ok(None) => {
                    return Err(BrokerError::Unreadable("pactl stopped subscribing".into()));
                }
                Ok(Some(line)) => {
                    if Self::WATCHED.iter().any(|watched| line.contains(watched)) {
                        return Ok(());
                    }
                }
                Err(error) => return Err(BrokerError::unreadable(error)),
            }
        }
    }

    async fn read() -> Result<AudioState, BrokerError> {
        let default = Self::run(&["get-default-sink"]).await?;
        let sinks = Self::run(&["--format=json", "list", "sinks"]).await?;
        Sinks::parse(&sinks, default.trim())
    }

    /// One `pactl` invocation, and its output.
    async fn run(args: &[&str]) -> Result<String, BrokerError> {
        let output = Command::new("pactl")
            .args(args)
            .kill_on_drop(true)
            .output()
            .await
            .map_err(BrokerError::unreadable)?;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).into_owned())
        } else {
            // Loud: a `pactl` that refused is not a machine with no sound.
            Err(BrokerError::Unreadable(format!(
                "pactl {}: {}",
                args.join(" "),
                String::from_utf8_lossy(&output.stderr).trim()
            )))
        }
    }
}

opaque_debug!(Link);

#[derive(Debug, Default)]
pub struct PipeWire {
    link: Option<Link>,
}

impl PipeWire {
    /// Resolve the default sink at command execution time.
    const DEFAULT: &'static str = "@DEFAULT_SINK@";

    pub fn new() -> Self {
        Self::default()
    }

    /// Convert a volume action to literal pactl arguments.
    pub fn arguments(change: &set_volume::Change) -> Vec<String> {
        match change {
            set_volume::Change::Absolute(level) => vec![
                "set-sink-volume".into(),
                Self::DEFAULT.into(),
                format!("{}%", (level.clamp(0.0, 1.0) * 100.0).round()),
            ],
            set_volume::Change::Delta(delta) => {
                let percent = (delta * 100.0).round();
                vec![
                    "set-sink-volume".into(),
                    Self::DEFAULT.into(),
                    // The sign has to be written even when it is positive:
                    // `pactl set-sink-volume 5%` sets it to five.
                    format!("{}{}%", if percent < 0.0 { "" } else { "+" }, percent),
                ]
            }
            set_volume::Change::ToggleMute(_) => vec![
                "set-sink-mute".into(),
                Self::DEFAULT.into(),
                "toggle".into(),
            ],
        }
    }

    fn patch(state: AudioState) -> StatePatch {
        StatePatch {
            topics: vec![StateTopic {
                topic: SystemTopic::Audio.as_str().into(),
                revision: 0, // the Hub assigns the real revision
                value: Some(state_topic::Value::Audio(state)),
            }],
        }
    }
}

#[async_trait]
impl Broker for PipeWire {
    fn name(&self) -> &'static str {
        "pipewire"
    }

    fn topics(&self) -> &'static [SystemTopic] {
        &[SystemTopic::Audio]
    }

    fn actions(&self) -> &'static [ActionKind] {
        &[ActionKind::SetVolume]
    }

    fn disconnect(&mut self) {
        self.link = None;
    }

    async fn connect(&mut self) -> Result<(), BrokerError> {
        self.link = Some(Link::open().await?);
        Ok(())
    }

    async fn wake(&mut self) -> Result<(), BrokerError> {
        self.link
            .as_mut()
            .ok_or_else(BrokerError::gone)?
            .wait()
            .await
    }

    async fn read(&mut self) -> Result<StatePatch, BrokerError> {
        Ok(Self::patch(Link::read().await?))
    }

    async fn act(&mut self, action: &action::Kind) -> Result<Option<StatePatch>, BrokerError> {
        let action::Kind::SetVolume(set) = action else {
            return Err(BrokerError::Unserved(ActionKind::of(action)));
        };
        let Some(change) = set.change.as_ref() else {
            return Err(BrokerError::Unreadable(
                "SetVolume carries no change".into(),
            ));
        };

        let arguments = Self::arguments(change);
        let borrowed: Vec<&str> = arguments.iter().map(String::as_str).collect();
        Link::run(&borrowed).await?;

        // The broker that just set it knows the new value, and the
        // subscription would take a moment to say so.
        Ok(Some(Self::patch(Link::read().await?)))
    }
}
