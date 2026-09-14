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
    /// `grim` is the Wayland screenshot tool; `slurp` selects a region and
    /// `wl-copy` puts the result on the clipboard. Named here rather than in
    /// the daemon because knowing them is a desktop convention.
    pub fn command(shot: &Screenshot) -> Option<String> {
        let mut grim = String::from("grim");

        if !shot.region_monitor_id.is_empty() {
            grim.push_str(&format!(" -o {}", Self::named(&shot.region_monitor_id)?));
        } else if !shot.fullscreen {
            // Neither a monitor nor the whole screen: ask where.
            grim.push_str(" -g \"$(slurp)\"");
        }

        Some(match (shot.clipboard, shot.output_path.is_empty()) {
            // grim writes to stdout when told to, which is what the clipboard
            // wants — and a path as well means both, which grim cannot do in
            // one pass.
            (true, true) => format!("{grim} - | wl-copy"),
            (true, false) => format!(
                "{grim} {path} && wl-copy < {path}",
                path = Self::named(&shot.output_path)?
            ),
            (false, false) => format!("{grim} {}", Self::named(&shot.output_path)?),
            // Nowhere to put it. grim's own default is a dated file in the
            // working directory, which for a daemon is not a place anyone
            // will find it.
            (false, true) => return None,
        })
    }

    /// A name that could be pasted into a shell line.
    ///
    /// These go through a shell — `grim … | wl-copy` is a pipeline — so a
    /// path carrying a quote or a semicolon would be a second command.
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
            // Loud: a screenshot nobody took is not a screenshot.
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
