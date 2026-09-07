use std::io;
use std::path::PathBuf;

use omega_core::{ReadError, TomlError, UnitName};
use omega_manifest::ManifestError;
use omega_wire::{CodecError, HandshakeError, Refusal};

#[derive(Debug, thiserror::Error)]
pub enum DaemonError {
    #[error("cannot bind control socket {}: {source}", path.display())]
    Bind {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("cannot load the state config: {0}")]
    Config(#[from] TomlError),
    #[error("cannot load unit manifests: {0}")]
    Manifests(#[from] ManifestStoreError),
    #[error("cannot load the state document: {0}")]
    Document(#[from] omega_document::DocumentError),
    #[error("{0}")]
    Shell(#[from] ShellError),
    #[error("io error: {0}")]
    Io(#[from] io::Error),
}

#[derive(Debug, thiserror::Error)]
#[error("unit {unit}: {source}")]
pub struct ManifestStoreError {
    pub unit: UnitName,
    #[source]
    pub source: ReadError<ManifestError>,
}

#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error("handshake failed: {0}")]
    Handshake(#[from] HandshakeError),
    #[error("transport error: {0}")]
    Transport(#[from] CodecError),
    /// The peer was turned away. It was told why before the socket closed.
    #[error("refused: {0}")]
    Refused(#[from] Refusal),
    /// The peer stopped answering keepalives.
    #[error("{0} stopped answering")]
    Unresponsive(String),
}

#[derive(Debug, thiserror::Error)]
pub enum ShellError {
    #[error("cannot bind shell socket {}: {source}", path.display())]
    Bind {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("io error: {0}")]
    Io(#[from] io::Error),
    #[error("cannot serialize view: {0}")]
    Encode(#[from] serde_json::Error),
}
