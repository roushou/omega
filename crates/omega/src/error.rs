//! What can go wrong for a plugin.

use omega_proto::{ClientError, CodecError, HandshakeError, Refusal};

/// The result of a plugin operation.
pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("effect failed: {0}")]
    Effect(#[from] crate::effect::EffectError),
    /// The daemon is not there, or would not have us.
    #[error("cannot reach the daemon: {0}")]
    Daemon(#[from] ClientError),
    #[error("transport error: {0}")]
    Transport(#[from] CodecError),
    /// The daemon turned this plugin away, or refused something it asked
    /// for. A refusal is an answer, not a broken connection: the code says
    /// whether the plugin was unknown, ungranted, or asked the impossible.
    #[error("the daemon refused: {0}")]
    Refused(#[from] Refusal),
    #[error(
        "protocol version mismatch: the daemon speaks v{peer}, this plugin speaks v{oldest}..=v{newest} — rebuild it against the running omega"
    )]
    VersionMismatch { peer: u32, oldest: u32, newest: u32 },
    /// A plugin's own name, or one of its surfaces', is not a name the
    /// system can address.
    #[error("{0:?} is not a usable name: {1}")]
    Name(String, #[source] omega_proto::IdentError),
    #[error("cannot start a runtime: {0}")]
    Runtime(#[source] std::io::Error),
    /// The build asked this plugin what it declares and could not read the
    /// answer.
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
