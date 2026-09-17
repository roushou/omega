//! Built-unit index written by the CLI and loaded by the daemon.

use std::path::PathBuf;

use crate::Layout;
use crate::{TomlFile, TomlSchema};
use omega_proto::UnitName;
use serde::{Deserialize, Serialize};

/// Built unit executable with a path relative to its generation.
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
    /// Filename shared by staged and published built-unit indexes.
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
    fn decode(source: &str) -> Result<Self, crate::TomlError> {
        crate::Toml::deserialize(source)
    }

    fn encode(&self) -> Result<String, crate::TomlError> {
        crate::Toml::serialize(self)
    }

    const KIND: &'static str = "state config";
    type Key<'a> = ();

    fn locate(layout: &Layout, _key: Self::Key<'_>) -> TomlFile<Self> {
        TomlFile::at(layout.state_units_toml())
    }
}
