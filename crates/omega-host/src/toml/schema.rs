use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::layout::Layout;
use crate::toml::file::TomlFile;

/// A TOML document's kind, location, and formatting.
/// Use [`Layout::file`] to locate a [`TomlFile`] for the schema.
pub trait TomlSchema: Serialize + DeserializeOwned + Sized {
    /// How the document is named in errors: "unit manifest", "cargo manifest".
    const KIND: &'static str;

    /// Dotted paths to tables whose entries use inline-table syntax,
    /// such as `omega = { workspace = true }`.
    const INLINE_ENTRIES: &'static [&'static str] = &[];

    /// Everything needed to address one instance. `()` for singletons; an
    /// enum when a schema has more than one home.
    type Key<'a>;

    fn locate(layout: &Layout, key: Self::Key<'_>) -> TomlFile<Self>;
}
