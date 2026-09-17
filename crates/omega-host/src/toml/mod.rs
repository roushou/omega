//! Typed TOML document access.
//!
//! - [`TomlSchema`] declares document names, paths, and codecs.
//! - [`TomlFile`] provides atomic read/write/edit operations at a typed path.
//! - [`TomlDoc`] retains a loaded value for edits spanning multiple steps.
//! - [`Toml`] encodes and decodes schema values.
//!
//! Each schema owns its codec; source documents retain their original formatting.

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
    /// Decode using the schema's codec.
    pub fn decode<S: TomlSchema>(src: &str) -> Result<S, TomlError> {
        S::decode(src)
    }

    /// Encode using the schema's codec.
    pub fn encode<S: TomlSchema>(value: &S) -> Result<String, TomlError> {
        value.encode()
    }

    /// Decode a data schema through Serde.
    pub fn deserialize<S: TomlSchema + serde::de::DeserializeOwned>(
        src: &str,
    ) -> Result<S, TomlError> {
        toml::from_str(src).map_err(|source| TomlError::Decode {
            kind: S::KIND,
            source: Box::new(source),
        })
    }

    /// Encode a data schema with deterministic table formatting.
    pub fn serialize<S: TomlSchema + serde::Serialize>(value: &S) -> Result<String, TomlError> {
        let mut document =
            toml_edit::ser::to_document(value).map_err(|source| TomlError::Encode {
                kind: S::KIND,
                source: Box::new(source),
            })?;
        Formatter::new(&[]).apply(&mut document);
        Ok(document.to_string())
    }
}
