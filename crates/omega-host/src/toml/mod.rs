//! Typed TOML document access.
//!
//! - [`TomlSchema`] declares document names, paths, and formatting.
//! - [`TomlFile`] provides atomic read/write/edit operations at a typed path.
//! - [`TomlDoc`] retains a loaded value for edits spanning multiple steps.
//! - [`Toml`] encodes and decodes schema values.
//!
//! [`Toml::encode`] applies [`TomlSchema::INLINE_ENTRIES`] for deterministic output.

mod doc;
mod error;
mod file;
mod format;
mod schema;
mod table;

use crate::toml::format::Formatter;

pub use doc::TomlDoc;
pub use error::TomlError;
pub use file::TomlFile;
pub use schema::TomlSchema;
pub use table::Table;

/// The TOML codec.
#[derive(Debug)]
pub struct Toml;

impl Toml {
    /// Decode a document from a string. The canonical inverse of
    /// [`encode`](Self::encode).
    pub fn decode<S: TomlSchema>(src: &str) -> Result<S, TomlError> {
        toml::from_str(src).map_err(|source| TomlError::Decode {
            kind: S::KIND,
            source: Box::new(source),
        })
    }

    /// Encode a document to its canonical string form, shaped by the
    /// schema's [`INLINE_ENTRIES`](TomlSchema::INLINE_ENTRIES).
    pub fn encode<S: TomlSchema>(value: &S) -> Result<String, TomlError> {
        let mut document =
            toml_edit::ser::to_document(value).map_err(|source| TomlError::Encode {
                kind: S::KIND,
                source: Box::new(source),
            })?;

        Formatter::new(S::INLINE_ENTRIES).apply(&mut document);
        Ok(document.to_string())
    }
}
