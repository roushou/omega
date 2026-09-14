//! Errors exposed to configuration authors.
use crate::{DocumentError, ValidationError};

/// The result of constructing or emitting an Omega configuration.
///
/// ```no_run
/// use omega_document::{Document, Result};
/// fn main() -> Result<()> {
///     Document::new().emit()
/// }
/// ```
pub type Result<T> = std::result::Result<T, Error>;

/// Concrete causes remain available to callers and diagnostic frontends.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Document(#[from] DocumentError),
    #[error("{0}")]
    Extension(#[source] Box<dyn std::error::Error + Send + Sync>),
    #[error(transparent)]
    Validation(#[from] ValidationError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}
