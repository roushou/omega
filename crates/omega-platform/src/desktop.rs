//! Screenshot, screen recording, and OCR capture through grim, slurp, wf-recorder,
//! tesseract, and the Wayland clipboard tools.

use async_trait::async_trait;
use omega_proto::omega::{
    CaptureText, RecordingConfig, Screenshot, StatePatch, action, record_screen,
};
use omega_proto::{ActionKind, SystemTopic};
use tokio::process::Command;

use crate::broker::{Broker, BrokerError, opaque_debug};

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

    /// Use grim and tesseract to OCR a selected region into the clipboard.
    pub fn text_command(capture: &CaptureText) -> Option<String> {
        let mut region = String::from("slurp");
        if !capture.region_monitor_id.is_empty() {
            region.push_str(&format!(" -o {}", Self::named(&capture.region_monitor_id)?));
        }
        Some(format!(
            "grim -g \"$({region})\" - | tesseract stdin stdout | wl-copy"
        ))
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

/// The active recording, owned by the broker until it stops.
#[derive(Default)]
pub struct Desktop {
    recording: Option<tokio::process::Child>,
}

opaque_debug!(Desktop);

impl Desktop {
    pub fn new() -> Self {
        Self::default()
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

    /// Ask for a region geometry; the user selects one.
    async fn region(monitor: &str) -> Result<String, BrokerError> {
        let mut command = Command::new("slurp");
        if !monitor.is_empty() {
            command.arg("-o").arg(monitor);
        }
        let output = command
            .kill_on_drop(true)
            .output()
            .await
            .map_err(BrokerError::unreadable)?;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
        } else {
            Err(BrokerError::Unreadable(format!(
                "slurp exited {}",
                output.status
            )))
        }
    }

    /// Spawn wf-recorder for the requested capture. Completion confirms the
    /// process started, not that it produced a file.
    async fn start_recording(
        config: &RecordingConfig,
    ) -> Result<tokio::process::Child, BrokerError> {
        let mut command = Command::new("wf-recorder");
        command.arg("-f").arg(&config.output_path);
        if config.fullscreen {
            if !config.region_monitor_id.is_empty() {
                command.arg("-o").arg(&config.region_monitor_id);
            }
        } else {
            command
                .arg("-g")
                .arg(Self::region(&config.region_monitor_id).await?);
        }
        if config.with_audio {
            command.arg("--audio");
        }

        command
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            // The broker owns the child's lifetime; dropping the handle must not
            // SIGKILL mid-recording. Stop and disconnect both interrupt it.
            .kill_on_drop(false)
            .spawn()
            .map_err(BrokerError::unreadable)
    }

    /// Ask the recording process to finish its file.
    fn interrupt(child: &tokio::process::Child) {
        if let Some(pid) = child.id() {
            // Safety: a live process id and a standard signal number.
            unsafe { libc::kill(pid as i32, libc::SIGINT) };
        }
    }

    async fn stop_recording(&mut self) -> Result<(), BrokerError> {
        let Some(mut child) = self.recording.take() else {
            return Err(BrokerError::Unreadable(
                "no screen recording is running".into(),
            ));
        };
        Self::interrupt(&child);
        child.wait().await.map_err(BrokerError::unreadable)?;
        Ok(())
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
        &[
            ActionKind::Screenshot,
            ActionKind::CaptureText,
            ActionKind::RecordScreen,
        ]
    }

    fn disconnect(&mut self) {
        // Interrupt without blocking; wf-recorder finalizes on SIGINT.
        if let Some(child) = self.recording.take() {
            Self::interrupt(&child);
        }
    }

    async fn act(&mut self, action: &action::Kind) -> Result<Option<StatePatch>, BrokerError> {
        match action {
            action::Kind::Screenshot(shot) => {
                let command = Capture::command(shot).ok_or_else(|| {
                    BrokerError::Unreadable("a screenshot needs somewhere to go".into())
                })?;
                Self::run(&command).await?;
            }
            action::Kind::CaptureText(capture) => {
                let command = Capture::text_command(capture)
                    .ok_or_else(|| BrokerError::Unreadable("OCR needs a valid region".into()))?;
                Self::run(&command).await?;
            }
            action::Kind::RecordScreen(record) => match &record.command {
                Some(record_screen::Command::Start(config)) => {
                    if self.recording.is_some() {
                        return Err(BrokerError::Unreadable(
                            "a screen recording is already running".into(),
                        ));
                    }
                    self.recording = Some(Self::start_recording(config).await?);
                }
                Some(record_screen::Command::Stop(_)) => self.stop_recording().await?,
                None => {
                    return Err(BrokerError::Unreadable(
                        "a recording command is required".into(),
                    ));
                }
            },
            other => return Err(BrokerError::Unserved(ActionKind::of(other))),
        }
        Ok(None)
    }
}
