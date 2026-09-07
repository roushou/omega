//! `units.toml`: the built state the daemon runs.
//!
//! Written by `omega build`, read by the daemon. It belongs to neither of
//! them — it is the on-disk contract between them, and it sits beside the
//! unit manifest it points at, because the two are what a build emits and
//! what the daemon reads back.

use std::path::PathBuf;

use omega_proto::{Layout, UnitName};
use omega_proto::{TomlFile, TomlSchema};
use serde::{Deserialize, Serialize};

/// A built unit the daemon should run. Paths are relative to the state dir,
/// so the whole directory can be moved or staged without rewriting it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BuiltUnit {
    pub name: UnitName,
    pub program: PathBuf,
    pub manifest: PathBuf,
}

impl BuiltUnit {
    /// The entry for `name`, with both relative paths taken from the layout.
    pub fn new(layout: &Layout, name: UnitName) -> Self {
        Self {
            program: layout.unit_program_rel(&name),
            manifest: layout.unit_manifest_rel(&name),
            name,
        }
    }
}

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct StateConfig {
    #[serde(default)]
    pub units: Vec<BuiltUnit>,
}

impl StateConfig {
    /// The file's name inside the state dir, so a staged copy and the final
    /// one cannot disagree.
    ///
    /// The layout places it; this is the same name, not a second one.
    pub const FILE_NAME: &'static str = Layout::UNITS_TOML;

    pub fn new(layout: &Layout, names: impl IntoIterator<Item = UnitName>) -> Self {
        Self {
            units: names
                .into_iter()
                .map(|name| BuiltUnit::new(layout, name))
                .collect(),
        }
    }

    pub fn names(&self) -> impl Iterator<Item = &UnitName> {
        self.units.iter().map(|unit| &unit.name)
    }
}

impl TomlSchema for StateConfig {
    const KIND: &'static str = "state config";
    type Key<'a> = ();

    fn locate(layout: &Layout, _key: Self::Key<'_>) -> TomlFile<Self> {
        TomlFile::at(layout.state_units_toml())
    }
}
