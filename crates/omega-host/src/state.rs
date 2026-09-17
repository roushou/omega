//! Built-plugin index written by the CLI and loaded by the daemon.

use std::path::PathBuf;

use crate::Layout;
use crate::{TomlFile, TomlSchema};
use omega_proto::PluginName;
use serde::{Deserialize, Serialize};

/// Built plugin executable with a path relative to its generation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BuiltPlugin {
    pub name: PluginName,
    pub program: PathBuf,
    pub manifest: PathBuf,
}

impl BuiltPlugin {
    /// The entry for `name`, with both relative paths taken from the layout.
    pub fn new(layout: &Layout, name: PluginName) -> Self {
        Self {
            program: layout.plugin_program_rel(&name),
            manifest: layout.plugin_manifest_rel(&name),
            name,
        }
    }
}

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StateConfig {
    #[serde(default)]
    pub plugins: Vec<BuiltPlugin>,
}

impl StateConfig {
    /// Filename shared by staged and published built-plugin indexes.
    pub const FILE_NAME: &'static str = Layout::PLUGINS_TOML;

    pub fn new(layout: &Layout, names: impl IntoIterator<Item = PluginName>) -> Self {
        Self {
            plugins: names
                .into_iter()
                .map(|name| BuiltPlugin::new(layout, name))
                .collect(),
        }
    }

    pub fn names(&self) -> impl Iterator<Item = &PluginName> {
        self.plugins.iter().map(|plugin| &plugin.name)
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
        TomlFile::at(layout.state_plugins_toml())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn obsolete_index_fields_are_rejected_instead_of_loading_no_plugins() {
        assert!(StateConfig::decode("units = []").is_err());
        assert!(
            StateConfig::decode("plugins = []")
                .unwrap()
                .plugins
                .is_empty()
        );
    }
}
