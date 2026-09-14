//! Plugin operation errors.

use omega_proto::{ClientError, CodecError, HandshakeError, Refusal};

/// The result of a plugin operation.
pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("effect failed: {0}")]
    Effect(#[from] crate::effect::EffectError),
    /// Connection or protocol failure while communicating with the daemon.
    #[error("cannot reach the daemon: {0}")]
    Daemon(#[from] ClientError),
    #[error("transport error: {0}")]
    Transport(#[from] CodecError),
    /// A daemon refusal, including its structured error code.
    #[error("the daemon refused: {0}")]
    Refused(#[from] Refusal),
    #[error(
        "protocol version mismatch: the daemon speaks v{peer}, this plugin speaks v{oldest}..=v{newest} — rebuild it against the running omega"
    )]
    VersionMismatch { peer: u32, oldest: u32, newest: u32 },
    /// Invalid plugin or surface identifier.
    #[error("{0:?} is not a usable name: {1}")]
    Name(String, #[source] omega_proto::IdentError),
    #[error("cannot start a runtime: {0}")]
    Runtime(#[source] std::io::Error),
    /// Failure to serialize or write the plugin manifest.
    #[error("cannot write the manifest: {0}")]
    Describe(#[source] std::io::Error),
}

impl From<HandshakeError> for Error {
    fn from(e: HandshakeError) -> Self {
        match e {
            HandshakeError::VersionMismatch {
                peer,
                oldest,
                newest,
            } => Self::VersionMismatch {
                peer,
                oldest,
                newest,
            },
            other => Self::Daemon(ClientError::Handshake(other)),
        }
    }
}

impl Error {
    /// Reject invalid command input.
    ///
    /// ```
    /// let error = omega::Error::invalid("expected a network name");
    /// assert!(error.to_string().contains("expected a network name"));
    /// ```
    pub fn invalid(message: impl Into<String>) -> Self {
        Self::Refused(Refusal::invalid(message))
    }

    pub(crate) fn refusal(&self) -> Refusal {
        match self {
            Self::Effect(error) => error.refusal(),
            Self::Refused(refusal) | Self::Daemon(ClientError::Refused(refusal)) => refusal.clone(),
            Self::Daemon(_) | Self::Transport(_) | Self::Io(_) => {
                Refusal::unavailable(self.to_string())
            }
            Self::Name(..) => Refusal::invalid(self.to_string()),
            Self::VersionMismatch { .. } | Self::Runtime(_) | Self::Describe(_) => {
                Refusal::precondition(self.to_string())
            }
        }
    }
}
