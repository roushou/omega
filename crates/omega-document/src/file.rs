use std::path::{Path, PathBuf};

use omega_host::{AtomicFile, Layout};
use omega_proto::omega::StateDocument;

/// The state document on disk.
///
/// Canonical protobuf JSON, not TOML: the document is generated, machine-read,
/// and committed for the bootstrap path, so its text form should be the one
/// the schema already defines rather than a second mapping to keep honest.
#[derive(Debug, Clone)]
pub struct DocumentFile {
    path: PathBuf,
}

impl DocumentFile {
    /// The name the document is stored under, inside the state dir and inside
    /// a build's staging directory alike.
    pub const FILE_NAME: &'static str = "document.json";

    /// The document of a built state dir.
    pub fn of(layout: &Layout) -> Self {
        Self::at(layout.state.join(Self::FILE_NAME))
    }

    pub fn at(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn exists(&self) -> bool {
        self.path.exists()
    }

    pub fn read(&self) -> Result<StateDocument, DocumentError> {
        let src = std::fs::read_to_string(&self.path).map_err(|source| DocumentError::Read {
            path: self.path.clone(),
            source,
        })?;
        serde_json::from_str(&src).map_err(|source| DocumentError::Parse {
            path: self.path.clone(),
            source,
        })
    }

    /// The document, or an empty one when the config declares none. A config
    /// without a `system/` crate is a valid config: every built unit runs and
    /// nothing else is claimed.
    pub fn read_or_default(&self) -> Result<StateDocument, DocumentError> {
        match self.read() {
            Err(e) if e.is_not_found() => Ok(StateDocument::default()),
            other => other,
        }
    }

    pub fn write(&self, document: &StateDocument) -> Result<(), DocumentError> {
        let encoded = Self::encode(document)?;
        AtomicFile::at(&self.path)
            .write(encoded.as_bytes())
            .map_err(|source| DocumentError::Write {
                path: self.path.clone(),
                source,
            })
    }

    /// Parse a document from the text a `system/` crate emitted.
    pub fn parse(src: &str) -> Result<StateDocument, DocumentError> {
        serde_json::from_str(src).map_err(DocumentError::Decode)
    }

    /// The canonical text form: stable key order and indentation, so a
    /// committed document diffs line by line.
    pub fn encode(document: &StateDocument) -> Result<String, DocumentError> {
        let mut encoded = serde_json::to_string_pretty(document).map_err(DocumentError::Encode)?;
        encoded.push('\n');
        Ok(encoded)
    }
}

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
