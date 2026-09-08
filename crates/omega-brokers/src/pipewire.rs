//! The volume, from PipeWire — through `pactl`.
//!
//! **Why a subprocess.** Talking to PipeWire natively means `libpipewire`
//! headers at build time, and a `cargo install omega-cli` that fails on a
//! machine without them is not an answer — the same reasoning that has
//! `build.rs` bundling protoc. `pactl` ships with pipewire-pulse, speaks
//! `--format=json`, and is PipeWire's own tool: using it is not
//! reimplementing PipeWire, which is what this daemon promises not to do.
//!
//! The cost is a process per reading. A volume is read when something changed
//! and not otherwise, so that is a handful a day.

use async_trait::async_trait;
use serde::Deserialize;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};

use omega_proto::omega::{AudioState, StatePatch, StateTopic, action, set_volume, state_topic};
use omega_proto::{ActionKind, SystemTopic};

use crate::broker::{Broker, BrokerError};

/// One sink, as `pactl --format=json list sinks` describes it.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct Sink {
    pub name: String,
    pub mute: bool,
    /// One entry per channel. They move together unless somebody has
    /// deliberately unbalanced them.
    pub volume: std::collections::HashMap<String, Channel>,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq)]
pub struct Channel {
    /// Raw PulseAudio volume, where 65536 is unattenuated.
    pub value: u32,
}

/// What `pactl` said, turned into the ontology.
///
/// Pure, and separate from running anything: the JSON is the part a test can
/// hold, and the conversion out of PulseAudio's scale is the part that can be
/// wrong.
#[derive(Debug)]
pub struct Sinks;

impl Sinks {
    /// `PA_VOLUME_NORM`: the raw value that means unattenuated.
    const NORM: f64 = 65536.0;

    pub fn parse(json: &str, default: &str) -> Result<AudioState, BrokerError> {
        let sinks: Vec<Sink> =
            serde_json::from_str(json).map_err(|error| BrokerError::Unreadable {
                subsystem: "pipewire",
                detail: error.to_string(),
            })?;
        Ok(Self::of(&sinks, default))
    }

    /// The default sink's reading, or silence where there is no such sink.
    ///
    /// A machine with no default sink reports muted at zero rather than an
    /// absent topic: PipeWire is answering, and "nothing is playing anywhere"
    /// is a reading.
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

    /// The loudest channel, as a fraction.
    ///
    /// The loudest rather than the average: channels move together in
    /// practice, and where they do not, one side at full volume is a machine
    /// that is loud. Clamped at one — PulseAudio allows amplification past
    /// unattenuated and the ontology does not.
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
    events: BufReader<tokio::process::ChildStdout>,
    /// Kept so the subscription is killed when this broker is dropped rather
    /// than outliving the daemon that started it.
    _child: Child,
}

impl Link {
    /// The events worth re-reading for. `pactl subscribe` reports every
    /// stream a browser opens; a broker that woke on all of them would run a
    /// process per notification sound.
    const WATCHED: &'static [&'static str] = &["on sink", "on server"];

    async fn open() -> Result<Self, BrokerError> {
        let mut child = Command::new("pactl")
            .arg("subscribe")
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(Self::unreadable)?;

        let stdout = child.stdout.take().ok_or(BrokerError::Unreadable {
            subsystem: "pipewire",
            detail: "pactl gave no output to read".into(),
        })?;

        Ok(Self {
            events: BufReader::new(stdout),
            _child: child,
        })
    }

    /// Wait for an event that changes what a volume widget draws.
    async fn wait(&mut self) -> Result<(), BrokerError> {
        loop {
            let mut line = String::new();
            match self.events.read_line(&mut line).await {
                Ok(0) => {
                    return Err(BrokerError::Unreadable {
                        subsystem: "pipewire",
                        detail: "pactl stopped subscribing".into(),
                    });
                }
                Ok(_) => {
                    if Self::WATCHED.iter().any(|watched| line.contains(watched)) {
                        return Ok(());
                    }
                }
                Err(error) => return Err(Self::unreadable(error)),
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
            .output()
            .await
            .map_err(Self::unreadable)?;

        match output.status.success() {
            true => Ok(String::from_utf8_lossy(&output.stdout).into_owned()),
            // Loud: a `pactl` that refused is not a machine with no sound.
            false => Err(BrokerError::Unreadable {
                subsystem: "pipewire",
                detail: format!(
                    "pactl {}: {}",
                    args.join(" "),
                    String::from_utf8_lossy(&output.stderr).trim()
                ),
            }),
        }
    }

    fn unreadable(error: impl std::fmt::Display) -> BrokerError {
        BrokerError::Unreadable {
            subsystem: "pipewire",
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
pub struct PipeWire {
    link: Option<Link>,
    primed: bool,
}

impl PipeWire {
    /// The sink every command names: whichever is default at the time, which
    /// is what a user means by "the volume".
    const DEFAULT: &'static str = "@DEFAULT_SINK@";

    pub fn new() -> Self {
        Self::default()
    }

    /// The `pactl` arguments for a volume change.
    ///
    /// Pure, because PulseAudio's spelling of a change and Omega's are not
    /// the same: a signed fraction becomes a percentage with a sign on the
    /// front, and getting the sign wrong turns every volume-down key up.
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

        let state = Link::read().await?;
        self.primed = true;
        Ok(Self::patch(state))
    }

    async fn act(&mut self, action: &action::Kind) -> Result<Option<StatePatch>, BrokerError> {
        let action::Kind::SetVolume(set) = action else {
            return Err(BrokerError::Unserved(ActionKind::of(action)));
        };
        let Some(change) = set.change.as_ref() else {
            return Err(BrokerError::Unreadable {
                subsystem: "pipewire",
                detail: "SetVolume carries no change".into(),
            });
        };

        let arguments = Self::arguments(change);
        let borrowed: Vec<&str> = arguments.iter().map(String::as_str).collect();
        Link::run(&borrowed).await?;

        // The broker that just set it knows the new value, and the
        // subscription would take a moment to say so.
        Ok(Some(Self::patch(Link::read().await?)))
    }
}
