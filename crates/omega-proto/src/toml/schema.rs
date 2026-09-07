use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::layout::Layout;
use crate::toml::file::TomlFile;

/// A kind of TOML document: what it is called, and where its instances live.
///
/// This is the single declaration of a document. Call sites never join paths
/// or name files; they ask [`Layout::file`] for a [`TomlFile`] and read it.
pub trait TomlSchema: Serialize + DeserializeOwned + Sized {
    /// How the document is named in errors: "unit manifest", "cargo manifest".
    const KIND: &'static str;

    /// Dotted paths to tables whose entries are written as inline tables —
    /// `omega = { workspace = true }` rather than a
    /// `[dependencies.omega]` section. Formatting is part of a document's
    /// declaration, not a decision at the call site.
    const INLINE_ENTRIES: &'static [&'static str] = &[];

    /// Everything needed to address one instance. `()` for singletons; an
    /// enum when a schema has more than one home.
    type Key<'a>;

    fn locate(layout: &Layout, key: Self::Key<'_>) -> TomlFile<Self>;
}

/// A schema with invariants that parsing alone cannot establish.
///
/// Implementing this puts a rule in one place; every reader gets it by
/// calling [`TomlFile::read_valid`].
pub trait Validated: TomlSchema {
    /// What the document is validated against (the unit it belongs to, say).
    type Context<'a>;
    type Invalid: std::error::Error + Send + Sync + 'static;

    fn validate(&self, cx: Self::Context<'_>) -> Result<(), Self::Invalid>;
}
