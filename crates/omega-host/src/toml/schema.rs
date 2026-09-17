use crate::layout::Layout;
use crate::toml::file::TomlFile;

/// A TOML document's kind, location, and codec.
/// Use [`Layout::file`] to locate a [`TomlFile`] for the schema.
pub trait TomlSchema: Sized {
    /// How the document is named in errors: "plugin manifest", "cargo manifest".
    const KIND: &'static str;

    /// Parse the document without reading the filesystem.
    fn decode(source: &str) -> Result<Self, super::TomlError>;

    /// Render the document, preserving source formatting when the schema supports it.
    fn encode(&self) -> Result<String, super::TomlError>;

    /// Everything needed to address one instance. `()` for singletons; an
    /// enum when a schema has more than one home.
    type Key<'a>;

    fn locate(layout: &Layout, key: Self::Key<'_>) -> TomlFile<Self>;
}
