//! Daemon-to-plugin request routing over active sessions.
//! Allocate even stream IDs; peers allocate odd IDs on the same connection.

use std::time::Duration;

use tokio::sync::{mpsc, oneshot};

use omega_proto::PluginName;
use omega_proto::Refusal;
use omega_proto::omega::{invoke, result};

/// One request to a plugin, and where its answer goes.
#[derive(Debug)]
pub struct Request {
    pub op: invoke::Op,
    pub(crate) _bytes: tokio::sync::OwnedSemaphorePermit,
    pub answer: oneshot::Sender<Result<result::Outcome, Refusal>>,
}

/// How long a plugin gets to answer before the daemon gives up on it. A plugin
/// that is wedged must not wedge the reconciler.
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, thiserror::Error)]
pub enum RequestError {
    #[error("request capacity for {0} exhausted")]
    Full(PluginName),
    #[error("request to {0} exceeds payload limit")]
    TooLarge(PluginName),
    #[error("{0} is not connected")]
    Absent(PluginName),
    #[error("{0} did not answer in time")]
    Timeout(PluginName),
    #[error("{plugin} refused: {source}")]
    Refused {
        plugin: PluginName,
        #[source]
        source: Refusal,
    },
}

#[derive(Debug, Clone)]
pub(crate) struct SessionLink {
    pub(crate) manifest: Option<std::sync::Arc<omega_proto::Manifest>>,
    pub(crate) bytes: std::sync::Arc<tokio::sync::Semaphore>,
    pub(crate) requests: tokio::sync::mpsc::Sender<Request>,
    pub(crate) stop: crate::Shutdown,
}

impl SessionLink {
    pub(crate) async fn request(
        &self,
        plugin: &PluginName,
        op: invoke::Op,
        timeout: std::time::Duration,
    ) -> Result<result::Outcome, RequestError> {
        let size = op.encoded_len();
        if size > omega_proto::MAX_FRAME_LEN - 32 {
            return Err(RequestError::TooLarge(plugin.clone()));
        }
        let bytes = self
            .bytes
            .clone()
            .try_acquire_many_owned(size as u32)
            .map_err(|_| RequestError::Full(plugin.clone()))?;
        let slot = self.requests.try_reserve().map_err(|error| match error {
            mpsc::error::TrySendError::Full(_) => RequestError::Full(plugin.clone()),
            mpsc::error::TrySendError::Closed(_) => RequestError::Absent(plugin.clone()),
        })?;
        let (answer, answered) = oneshot::channel();
        let deadline = tokio::time::Instant::now() + timeout;
        slot.send(Request {
            op,
            answer,
            _bytes: bytes,
        });

        match tokio::time::timeout_at(deadline, answered).await {
            Ok(Ok(Ok(outcome))) => Ok(outcome),
            Ok(Ok(Err(refusal))) => Err(RequestError::Refused {
                plugin: plugin.clone(),
                source: refusal,
            }),
            // The session ended while the request was outstanding.
            Ok(Err(_)) => Err(RequestError::Absent(plugin.clone())),
            Err(_) => Err(RequestError::Timeout(plugin.clone())),
        }
    }
}
