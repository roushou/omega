//! Read and write the clipboard through wl-paste and wl-copy. History is not
//! provided; the reading is the current plain-text content only.

use std::time::Duration;

use async_trait::async_trait;
use tokio::process::Command;
use tokio::time::timeout;

use omega_proto::omega::{ClipboardState, StatePatch, StateTopic, action, state_topic};
use omega_proto::{ActionKind, SystemTopic};

use crate::broker::{Broker, BrokerError, Cadence};

#[derive(Debug, Default)]
pub struct Clipboard {}

impl Clipboard {
    pub fn new() -> Self {
        Self::default()
    }

    /// How often the clipboard is polled while nothing else changes.
    const POLL: Duration = Duration::from_secs(2);

    /// How long a single read waits for content before reporting empty.
    const READ_TIMEOUT: Duration = Duration::from_millis(500);

    async fn paste() -> Result<String, BrokerError> {
        let output = Command::new("wl-paste")
            .arg("--no-newline")
            .kill_on_drop(true)
            .output()
            .await
            .map_err(BrokerError::unreadable)?;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).into_owned())
        } else {
            Err(BrokerError::Unreadable(format!(
                "wl-paste exited {}",
                output.status
            )))
        }
    }

    fn patch(text: String) -> StatePatch {
        StatePatch {
            topics: vec![StateTopic {
                topic: SystemTopic::Clipboard.as_str().into(),
                revision: 0, // the Hub assigns the real revision
                value: Some(state_topic::Value::Clipboard(ClipboardState { text })),
            }],
        }
    }

    async fn write(text: &str) -> Result<(), BrokerError> {
        let mut child = Command::new("wl-copy")
            .stdin(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(BrokerError::unreadable)?;

        use tokio::io::AsyncWriteExt;
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| BrokerError::Unreadable("wl-copy gave no input to write".into()))?;
        stdin
            .write_all(text.as_bytes())
            .await
            .map_err(BrokerError::unreadable)?;
        drop(stdin);

        let status = child.wait().await.map_err(BrokerError::unreadable)?;
        if status.success() {
            Ok(())
        } else {
            Err(BrokerError::Unreadable(format!("wl-copy exited {status}")))
        }
    }

    async fn clear() -> Result<(), BrokerError> {
        let status = Command::new("wl-copy")
            .arg("--clear")
            .kill_on_drop(true)
            .status()
            .await
            .map_err(BrokerError::unreadable)?;

        if status.success() {
            Ok(())
        } else {
            Err(BrokerError::Unreadable(format!(
                "wl-copy --clear exited {status}"
            )))
        }
    }
}

#[async_trait]
impl Broker for Clipboard {
    fn name(&self) -> &'static str {
        "clipboard"
    }

    fn topics(&self) -> &'static [SystemTopic] {
        &[SystemTopic::Clipboard]
    }

    fn actions(&self) -> &'static [ActionKind] {
        &[ActionKind::WriteClipboard, ActionKind::ClearClipboard]
    }

    async fn wake(&mut self) -> Result<(), BrokerError> {
        Cadence::after(Self::POLL).wait().await;
        Ok(())
    }

    async fn read(&mut self) -> Result<StatePatch, BrokerError> {
        // An empty or absent selection is a real reading, not a failure; a
        // missing tool or a stuck client remains an error and reconnects.
        let text = match timeout(Self::READ_TIMEOUT, Self::paste()).await {
            Ok(Ok(text)) => text,
            Ok(Err(_)) | Err(_) => String::new(),
        };
        Ok(Self::patch(text))
    }

    async fn act(&mut self, action: &action::Kind) -> Result<Option<StatePatch>, BrokerError> {
        match action {
            action::Kind::WriteClipboard(write) => Self::write(&write.text).await?,
            action::Kind::ClearClipboard(_) => Self::clear().await?,
            other => return Err(BrokerError::Unserved(ActionKind::of(other))),
        }
        Ok(None)
    }
}
