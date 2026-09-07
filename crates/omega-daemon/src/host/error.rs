//! Errors the host-side utilities return.

use std::path::PathBuf;

use omega_proto::{IdentError, TomlError};

#[derive(Debug, thiserror::Error)]
pub enum PatternError {
    #[error("cannot expand {pattern:?} in {}: {source}", dir.display())]
    Expand {
        pattern: String,
        dir: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum UnitsError {
    #[error(transparent)]
    Manifest(#[from] TomlError),
    #[error(transparent)]
    Pattern(#[from] PatternError),
    #[error(transparent)]
    Name(#[from] IdentError),
    #[error("workspace member has no directory name: {}", .0.display())]
    UnnamedMember(PathBuf),
}
