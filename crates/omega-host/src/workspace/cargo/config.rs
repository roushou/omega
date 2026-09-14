//! Machine-local Cargo overrides.
use super::Dependencies;
use crate::Table;
use crate::{Layout, TomlFile, TomlSchema};
use serde::{Deserialize, Serialize};

/// `.cargo/config.toml` — cargo's own configuration for a directory tree.
///
/// One thing lives here and it is deliberate: `[patch]`, the override that
/// points a dependency at a checkout. A manifest is committed and has to
/// resolve on every machine that clones it, so it names published crates; a
/// path into somebody's home directory belongs in a file that stays on that
/// machine.
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct CargoConfig {
    /// Source name (`crates-io`) to the crates replaced within it.
    #[serde(default, skip_serializing_if = "Table::is_empty")]
    pub patch: Table<Dependencies>,
    /// Everything cargo understands and omega does not.
    #[serde(flatten)]
    pub rest: Table<toml::Value>,
}

impl CargoConfig {
    /// The source name a crates.io dependency is patched under.
    pub const REGISTRY: &'static str = "crates-io";

    /// The crates currently replaced by a local path.
    pub fn patched(&self) -> Option<&Dependencies> {
        self.patch.get(Self::REGISTRY)
    }

    /// Replace the whole patch table: linking is a statement about where the
    /// crates come from, not an addition to previous statements.
    pub fn replace_patch(&mut self, patched: Dependencies) {
        if patched.is_empty() {
            self.patch.remove(Self::REGISTRY);
        } else {
            self.patch.insert(Self::REGISTRY, patched);
        }
    }

    pub fn is_empty(&self) -> bool {
        self.patch.is_empty() && self.rest.is_empty()
    }
}

impl TomlSchema for CargoConfig {
    const KIND: &'static str = "cargo config";
    type Key<'a> = ();

    fn locate(layout: &Layout, _key: Self::Key<'_>) -> TomlFile<Self> {
        TomlFile::at(layout.cargo_config())
    }
}
