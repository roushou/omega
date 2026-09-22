//! Read and control audio sinks, sources, and per-application streams through
//! pactl. A persistent subscription triggers JSON queries after audio changes.

use std::collections::HashMap;

use async_trait::async_trait;
use serde::Deserialize;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};

use omega_proto::omega::{
    AudioSinksState, AudioState, AudioStream, AudioStreamsState, SinkInfo, StatePatch, StateTopic,
    action, set_volume, state_topic,
};
use omega_proto::{ActionKind, SystemTopic};

use crate::broker::{Broker, BrokerError, opaque_debug};

/// One sink or source, as `pactl --format=json list sinks` describes it.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct Device {
    pub name: String,
    pub mute: bool,
    /// Human-readable description, as PulseAudio reports it.
    #[serde(default)]
    pub description: String,
    /// Per-channel volume values.
    #[serde(default)]
    pub volume: HashMap<String, Channel>,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq)]
pub struct Channel {
    /// Raw PulseAudio volume, where 65536 is unattenuated.
    pub value: u32,
}

/// One application stream, as `pactl --format=json list sink-inputs` describes it.
#[derive(Debug, Clone, Deserialize)]
pub struct SinkInput {
    pub index: u32,
    pub mute: bool,
    #[serde(default)]
    pub volume: HashMap<String, Channel>,
    #[serde(rename = "application.name", default)]
    pub application_name: String,
}

/// `PA_VOLUME_NORM`: the raw value that means unattenuated.
const NORM: f64 = 65536.0;

/// Return the loudest channel as a fraction, clamped to 1.0.
fn level(volume: &HashMap<String, Channel>) -> f64 {
    volume
        .values()
        .map(|channel| f64::from(channel.value) / NORM)
        .fold(0.0, f64::max)
        .clamp(0.0, 1.0)
}

/// Decode pactl JSON and convert PulseAudio volume units.
#[derive(Debug)]
pub struct Sinks;

impl Sinks {
    pub fn parse(json: &str, default: &str) -> Result<AudioState, BrokerError> {
        let sinks: Vec<Device> = serde_json::from_str(json)
            .map_err(|error| BrokerError::Unreadable(error.to_string()))?;
        Ok(Self::of(&sinks, default))
    }

    /// The available sinks, as names and descriptions.
    pub fn catalogue(sinks: &[Device]) -> Vec<SinkInfo> {
        sinks
            .iter()
            .map(|sink| SinkInfo {
                name: sink.name.clone(),
                description: sink.description.clone(),
            })
            .collect()
    }

    /// Read the default sink. If no sink exists, report zero volume and muted state.
    pub fn of(sinks: &[Device], default: &str) -> AudioState {
        let Some(sink) = sinks.iter().find(|sink| sink.name == default) else {
            return AudioState {
                volume: 0.0,
                muted: true,
                default_sink: default.to_string(),
                input_volume: 0.0,
                input_muted: true,
                default_source: String::new(),
            };
        };

        AudioState {
            volume: level(&sink.volume),
            muted: sink.mute,
            default_sink: sink.name.clone(),
            input_volume: 0.0,
            input_muted: true,
            default_source: String::new(),
        }
    }
}

/// Decode the default source's input level and mute state.
#[derive(Debug)]
pub struct Sources;

impl Sources {
    pub fn parse(json: &str, default: &str) -> Result<(f64, bool), BrokerError> {
        let sources: Vec<Device> = serde_json::from_str(json)
            .map_err(|error| BrokerError::Unreadable(error.to_string()))?;
        Ok(Self::of(&sources, default))
    }

    /// Read the default source. If none exists, report silence and muted.
    pub fn of(sources: &[Device], default: &str) -> (f64, bool) {
        match sources.iter().find(|source| source.name == default) {
            Some(source) => (level(&source.volume), source.mute),
            None => (0.0, true),
        }
    }
}

/// Decode per-application streams.
#[derive(Debug)]
pub struct Streams;

impl Streams {
    pub fn parse(json: &str) -> Result<Vec<AudioStream>, BrokerError> {
        let inputs: Vec<SinkInput> = serde_json::from_str(json)
            .map_err(|error| BrokerError::Unreadable(error.to_string()))?;
        Ok(inputs
            .into_iter()
            .map(|input| AudioStream {
                index: input.index,
                app: input.application_name,
                volume: level(&input.volume),
                muted: input.mute,
            })
            .collect())
    }
}

/// `pactl`, and the subscription held open to it.
struct Link {
    events: tokio::io::Lines<BufReader<tokio::process::ChildStdout>>,
    /// Dropping the broker must terminate its subscription process.
    _child: Child,
}

impl Link {
    /// Refresh for sink, source, stream, and server changes.
    const WATCHED: &'static [&'static str] =
        &["on sink", "on sink-input", "on source", "on server"];

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

    /// Wait for an audio state change.
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

    async fn read_sinks() -> Result<(AudioState, Vec<SinkInfo>), BrokerError> {
        let default = Self::run(&["get-default-sink"]).await?;
        let json = Self::run(&["--format=json", "list", "sinks"]).await?;
        let sinks: Vec<Device> = serde_json::from_str(&json)
            .map_err(|error| BrokerError::Unreadable(error.to_string()))?;
        Ok((Sinks::of(&sinks, default.trim()), Sinks::catalogue(&sinks)))
    }

    async fn read_sources() -> Result<(f64, bool), BrokerError> {
        let default = Self::run(&["get-default-source"]).await?;
        let sources = Self::run(&["--format=json", "list", "sources"]).await?;
        Sources::parse(&sources, default.trim())
    }

    async fn read_streams() -> Result<Vec<AudioStream>, BrokerError> {
        let inputs = Self::run(&["--format=json", "list", "sink-inputs"]).await?;
        Streams::parse(&inputs)
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
    /// Resolve the default devices at command execution time.
    const DEFAULT_SINK: &'static str = "@DEFAULT_SINK@";
    const DEFAULT_SOURCE: &'static str = "@DEFAULT_SOURCE@";

    pub fn new() -> Self {
        Self::default()
    }

    /// Convert a volume action to literal pactl arguments.
    pub fn arguments(change: &set_volume::Change) -> Vec<String> {
        match change {
            set_volume::Change::Absolute(level) => vec![
                "set-sink-volume".into(),
                Self::DEFAULT_SINK.into(),
                format!("{}%", (level.clamp(0.0, 1.0) * 100.0).round()),
            ],
            set_volume::Change::Delta(delta) => {
                let percent = (delta * 100.0).round();
                vec![
                    "set-sink-volume".into(),
                    Self::DEFAULT_SINK.into(),
                    // The sign has to be written even when it is positive:
                    // `pactl set-sink-volume 5%` sets it to five.
                    format!("{}{}%", if percent < 0.0 { "" } else { "+" }, percent),
                ]
            }
            set_volume::Change::ToggleMute(_) => vec![
                "set-sink-mute".into(),
                Self::DEFAULT_SINK.into(),
                "toggle".into(),
            ],
            set_volume::Change::Muted(muted) => vec![
                "set-sink-mute".into(),
                Self::DEFAULT_SINK.into(),
                if *muted { "1" } else { "0" }.into(),
            ],
        }
    }

    async fn read_all() -> Result<StatePatch, BrokerError> {
        let (mut audio, sinks) = Link::read_sinks().await?;
        let (input_volume, input_muted) = Link::read_sources().await?;
        audio.input_volume = input_volume;
        audio.input_muted = input_muted;
        audio.default_source = Link::run(&["get-default-source"]).await?.trim().into();
        let streams = Link::read_streams().await?;
        Ok(Self::patch(audio, streams, sinks))
    }

    fn patch(audio: AudioState, streams: Vec<AudioStream>, sinks: Vec<SinkInfo>) -> StatePatch {
        StatePatch {
            topics: vec![
                StateTopic {
                    topic: SystemTopic::Audio.as_str().into(),
                    revision: 0, // the Hub assigns the real revision
                    value: Some(state_topic::Value::Audio(audio)),
                },
                StateTopic {
                    topic: SystemTopic::AudioStreams.as_str().into(),
                    revision: 0,
                    value: Some(state_topic::Value::AudioStreams(AudioStreamsState {
                        streams,
                    })),
                },
                StateTopic {
                    topic: SystemTopic::AudioSinks.as_str().into(),
                    revision: 0,
                    value: Some(state_topic::Value::AudioSinks(AudioSinksState { sinks })),
                },
            ],
        }
    }
}

#[async_trait]
impl Broker for PipeWire {
    fn name(&self) -> &'static str {
        "pipewire"
    }

    fn topics(&self) -> &'static [SystemTopic] {
        &[
            SystemTopic::Audio,
            SystemTopic::AudioStreams,
            SystemTopic::AudioSinks,
        ]
    }

    fn actions(&self) -> &'static [ActionKind] {
        &[
            ActionKind::SetVolume,
            ActionKind::SetStreamVolume,
            ActionKind::SetStreamMute,
            ActionKind::SetDefaultSink,
            ActionKind::SetInputMute,
            ActionKind::SetInputVolume,
        ]
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
        Self::read_all().await
    }

    async fn act(&mut self, action: &action::Kind) -> Result<Option<StatePatch>, BrokerError> {
        match action {
            action::Kind::SetVolume(set) => {
                let Some(change) = set.change.as_ref() else {
                    return Err(BrokerError::Unreadable(
                        "SetVolume carries no change".into(),
                    ));
                };
                let arguments = Self::arguments(change);
                let borrowed: Vec<&str> = arguments.iter().map(String::as_str).collect();
                Link::run(&borrowed).await?;
            }
            action::Kind::SetStreamVolume(set) => {
                let percent = format!("{}%", (set.absolute.clamp(0.0, 1.0) * 100.0).round());
                Link::run(&[
                    "set-sink-input-volume",
                    &set.stream_index.to_string(),
                    &percent,
                ])
                .await?;
            }
            action::Kind::SetStreamMute(set) => {
                Link::run(&[
                    "set-sink-input-mute",
                    &set.stream_index.to_string(),
                    if set.muted { "1" } else { "0" },
                ])
                .await?;
            }
            action::Kind::SetDefaultSink(set) => {
                Link::run(&["set-default-sink", &set.sink_name]).await?;
            }
            action::Kind::SetInputMute(set) => {
                Link::run(&[
                    "set-source-mute",
                    Self::DEFAULT_SOURCE,
                    if set.muted { "1" } else { "0" },
                ])
                .await?;
            }
            action::Kind::SetInputVolume(set) => {
                let percent = format!("{}%", (set.absolute.clamp(0.0, 1.0) * 100.0).round());
                Link::run(&["set-source-volume", Self::DEFAULT_SOURCE, &percent]).await?;
            }
            other => return Err(BrokerError::Unserved(ActionKind::of(other))),
        }

        // The broker that just set it knows the new values, and the
        // subscription would take a moment to say so.
        Ok(Some(Self::read_all().await?))
    }
}
