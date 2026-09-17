//! What speaking TOML can fail with.

use std::path::PathBuf;

/// TOML operation errors with schema kind and file context.
/// Parser errors are boxed to bound the size of the error variant.
#[derive(Debug, thiserror::Error)]
pub enum TomlError {
    #[error("cannot read {kind} {}: {source}", path.display())]
    Read {
        kind: &'static str,
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("cannot parse {kind} {}: {source}", path.display())]
    Parse {
        kind: &'static str,
        path: PathBuf,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },
    #[error("cannot write {kind} {}: {source}", path.display())]
    Write {
        kind: &'static str,
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("cannot decode {kind}: {source}")]
    Decode {
        kind: &'static str,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },
    #[error("cannot encode {kind}: {source}")]
    Encode {
        kind: &'static str,
        #[source]
        source: Box<toml_edit::ser::Error>,
    },
}

impl TomlError {
    pub(super) fn at_path(self, path: &std::path::Path) -> Self {
        match self {
            Self::Decode { kind, source } => Self::Parse {
                kind,
                path: path.to_path_buf(),
                source,
            },
            other => other,
        }
    }

    pub fn is_not_found(&self) -> bool {
        matches!(self, Self::Read { source, .. } if source.kind() == std::io::ErrorKind::NotFound)
    }

    /// The file the operation was against, for the variants that have one.
    pub fn path(&self) -> Option<&std::path::Path> {
        match self {
            Self::Read { path, .. } | Self::Parse { path, .. } | Self::Write { path, .. } => {
                Some(path)
            }
            Self::Decode { .. } | Self::Encode { .. } => None,
        }
    }
}
