//! Every error this crate returns.

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

/// A name that does not obey the one rule every omega identifier follows.
/// The kind is carried so the message says what was being named.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum IdentError {
    #[error("{kind} must be lowercase letters, digits, hyphens and underscores: {name:?}")]
    InvalidCharacters { kind: &'static str, name: String },
    #[error("{kind} must start with a lowercase letter: {name:?}")]
    InvalidStart { kind: &'static str, name: String },
}
