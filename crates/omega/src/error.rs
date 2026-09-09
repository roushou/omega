//! What can go wrong for a plugin.

use omega_proto::{ClientError, CodecError, HandshakeError, Refusal};

/// The result of running a plugin.
pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
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
