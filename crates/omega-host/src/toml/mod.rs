//! TOML, spoken through declared schemas.
//!
//! Four types, address to bytes:
//!  - [`TomlSchema`]: a document kind — its name, and where instances live.
//!    Implemented once per document; [`Layout::file`](crate::Layout::file)
//!    turns a schema plus a key into a [`TomlFile`].
//!  - [`TomlFile<S>`]: a typed address. Cheap and I/O-free until asked to
//!    read, write, or edit. Writes are atomic.
//!  - [`TomlDoc<S>`]: a loaded document that remembers where it came from,
//!    for when a value has to outlive the call that loaded it. `TomlFile`
//!    covers a read, a write, or a read-modify-write; `TomlDoc` covers
//!    everything that happens across several steps before one save.
//!  - [`Toml`]: the codec, and the only place `toml`/`toml_edit` are named.
//!
//! [`Toml::encode`] is the canonical form: the bytes written to disk and the
//! bytes hashed by a manifest come from it, so a hash cannot disagree with
//! the file it names. It writes through `toml_edit`, so a schema can declare
//! the shape it wants — see [`TomlSchema::INLINE_ENTRIES`].

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
