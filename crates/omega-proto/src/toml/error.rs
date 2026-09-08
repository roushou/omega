//! What speaking TOML can fail with.

use std::path::PathBuf;

/// A TOML operation that failed, labelled with the schema's
/// [`KIND`](crate::TomlSchema::KIND) and, where one exists, the file.
///
/// `toml`'s own errors are ~130 bytes (they carry the offending source text),
/// so they are boxed: a `Result<T, TomlError>` is returned from every TOML
/// call site and a large `Err` is paid for on the success path too.
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
        source: Box<toml::de::Error>,
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
        source: Box<toml::de::Error>,
    },
    #[error("cannot encode {kind}: {source}")]
    Encode {
        kind: &'static str,
        #[source]
        source: Box<toml_edit::ser::Error>,
    },
}

impl TomlError {
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

/// A document that parsed but does not satisfy its schema's invariants.
#[derive(Debug, thiserror::Error)]
pub enum ReadError<E>
where
    E: std::error::Error + 'static,
{
    #[error(transparent)]
    Toml(#[from] TomlError),
    #[error(transparent)]
    Invalid(E),
}
