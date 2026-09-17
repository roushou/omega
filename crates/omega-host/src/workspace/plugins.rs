//! Discover runnable plugins from config workspace members and globs.

use crate::TomlError;
use crate::cargo::{CargoError, CargoSlot, Manifest, MembersError};
use crate::{
    Layout,
    workspace::{WorkspaceError, WorkspaceRole},
};
use omega_proto::PluginName;

/// The runnable plugins of a config workspace, in deterministic (sorted) order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Plugins(Vec<PluginName>);

impl Plugins {
    /// Read the workspace manifest and derive its plugins: member directories
    /// directly under `plugins/`, minus `exclude`.
    pub fn discover(layout: &Layout) -> Result<Self, PluginsError> {
        let manifest = layout.file::<Manifest>(CargoSlot::Workspace).read()?;
        let workspace = manifest.workspace()?.ok_or_else(|| CargoError {
            field: "workspace".into(),
            reason: "is required".into(),
        })?;
        let mut names = Vec::new();
        for dir in workspace.member_dirs(&layout.config)? {
            match WorkspaceRole::at(layout, &dir)? {
                WorkspaceRole::Plugin(name) => names.push(name.plugin().clone()),
                WorkspaceRole::System | WorkspaceRole::Library(_) => {}
            }
        }

        names.sort();
        names.dedup();
        Ok(Self(names))
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn iter(&self) -> impl Iterator<Item = &PluginName> {
        self.0.iter()
    }
}

impl IntoIterator for Plugins {
    type Item = PluginName;
    type IntoIter = std::vec::IntoIter<PluginName>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a> IntoIterator for &'a Plugins {
    type Item = &'a PluginName;
    type IntoIter = std::slice::Iter<'a, PluginName>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

/// Why runnable plugins could not be discovered.
#[derive(Debug, thiserror::Error)]
pub enum PluginsError {
    #[error(transparent)]
    Workspace(#[from] WorkspaceError),
    #[error(transparent)]
    Manifest(#[from] TomlError),
    #[error(transparent)]
    Members(#[from] MembersError),
    #[error(transparent)]
    Cargo(#[from] CargoError),
}
