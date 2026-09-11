//! Errors exposed to configuration authors.
use crate::{DocumentError, ValidationError, shell::ShellError};

/// The result of constructing or emitting an Omega configuration.
///
/// ```no_run
/// use omega_document::{Document, Result};
/// use omega_document::shell::Shell;
/// fn main() -> Result<()> {
///     Document::new().shell(Shell::new())?.emit()
/// }
/// ```
pub type Result<T> = std::result::Result<T, Error>;

/// Concrete causes remain available to callers and diagnostic frontends.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Document(#[from] DocumentError),
    #[error(transparent)]
    Shell(#[from] ShellError),
    #[error(transparent)]
    Validation(#[from] ValidationError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}
