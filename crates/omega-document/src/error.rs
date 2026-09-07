use std::path::PathBuf;

/// Why a state document could not be read, written, or understood.
#[derive(Debug, thiserror::Error)]
pub enum DocumentError {
    #[error("cannot read the state document {}: {source}", path.display())]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("cannot parse the state document {}: {source}", path.display())]
    Parse {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("cannot write the state document {}: {source}", path.display())]
    Write {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("cannot decode a state document: {0}")]
    Decode(#[source] serde_json::Error),
    #[error("cannot encode a state document: {0}")]
    Encode(#[source] serde_json::Error),
}

impl DocumentError {
    /// True when there simply is no document — a config that has not declared
    /// one yet, which is not an error to the daemon.
    pub fn is_not_found(&self) -> bool {
        matches!(self, Self::Read { source, .. } if source.kind() == std::io::ErrorKind::NotFound)
    }
}
