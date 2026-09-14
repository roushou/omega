//! Screenshot capture.
use crate::broker::{Broker, BrokerError};
use async_trait::async_trait;
use omega_proto::omega::{Screenshot, StatePatch, action};
use omega_proto::{ActionKind, SystemTopic};
use tokio::process::Command;

/// A screenshot request, as a command line.
#[derive(Debug)]
pub struct Capture;

impl Capture {
    /// Use grim for capture, slurp for region selection, and wl-copy for clipboard output.
    pub fn command(shot: &Screenshot) -> Option<String> {
        let mut grim = String::from("grim");

        if !shot.region_monitor_id.is_empty() {
            grim.push_str(&format!(" -o {}", Self::named(&shot.region_monitor_id)?));
        } else if !shot.fullscreen {
            // Interactive capture requires selecting a region.
            grim.push_str(" -g \"$(slurp)\"");
        }

        Some(match (shot.clipboard, shot.output_path.is_empty()) {
            // Clipboard capture uses stdout and cannot also write a path in the same invocation.
            (true, true) => format!("{grim} - | wl-copy"),
            (true, false) => format!(
                "{grim} {path} && wl-copy < {path}",
                path = Self::named(&shot.output_path)?
            ),
            (false, false) => format!("{grim} {}", Self::named(&shot.output_path)?),
            // Require a destination instead of using the daemon's working directory.
            (false, true) => return None,
        })
    }

    /// Quote arguments before inserting them into a shell pipeline.
    fn named(name: &str) -> Option<&str> {
        if name.is_empty() || name.contains(['\'', '"', ';', '&', '|', '$', '`', '\n']) {
            None
        } else {
            Some(name)
        }
    }
}

#[derive(Debug, Default)]
pub struct Desktop;

impl Desktop {
    pub fn new() -> Self {
        Self
    }

    /// Run a command line, and wait long enough to know it started.
    async fn run(command: &str) -> Result<(), BrokerError> {
        let status = Command::new("sh")
            .arg("-c")
            .arg(command)
            .status()
            .await
            .map_err(|error| BrokerError::Unreadable(error.to_string()))?;

        if status.success() {
            Ok(())
        } else {
            Err(BrokerError::unreadable(format!(
                "{command:?} exited {status}"
            )))
        }
    }
}

#[async_trait]
impl Broker for Desktop {
    fn name(&self) -> &'static str {
        "desktop"
    }

    fn topics(&self) -> &'static [SystemTopic] {
        &[]
    }

    fn actions(&self) -> &'static [ActionKind] {
        &[ActionKind::Screenshot]
    }

    async fn act(&mut self, action: &action::Kind) -> Result<Option<StatePatch>, BrokerError> {
        let command = match action {
            action::Kind::Screenshot(shot) => Capture::command(shot).ok_or(
                BrokerError::Unreadable("a screenshot needs somewhere to go".into()),
            )?,
            other => return Err(BrokerError::Unserved(ActionKind::of(other))),
        };

        Self::run(&command).await?;
        Ok(None)
    }
}
