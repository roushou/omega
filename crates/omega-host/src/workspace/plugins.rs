//! Runnable plugins declared by the config workspace.
//!
//! The workspace manifest is the source of truth: its `members` globs name
//! the unit crates under `plugins/`. Plugins are derived from it, so there is no
//! separate registry that can drift out of sync with the crates.

use crate::TomlError;
use crate::workspace::cargo::{CargoManifest, CargoSlot};
use crate::workspace::pattern::PatternError;
use crate::{
    Layout,
    workspace::{WorkspaceError, WorkspaceRole},
};
use omega_proto::UnitName;

/// The runnable plugins of a config workspace, in deterministic (sorted) order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Plugins(Vec<UnitName>);

impl Plugins {
    /// Read the workspace manifest and derive its units: member directories
    /// directly under `plugins/`, minus `exclude`.
    pub fn discover(layout: &Layout) -> Result<Self, PluginsError> {
        WorkspaceRole::check_layout(layout)?;
        let manifest = layout.file::<CargoManifest>(CargoSlot::Workspace).read()?;
        let workspace = manifest.workspace.unwrap_or_default();
        let mut names = Vec::new();
        for dir in workspace.member_dirs(&layout.config)? {
            match WorkspaceRole::at(layout, &dir)? {
                WorkspaceRole::Plugin(name) => names.push(name.unit().clone()),
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

    pub fn iter(&self) -> impl Iterator<Item = &UnitName> {
        self.0.iter()
    }
}

impl IntoIterator for Plugins {
    type Item = UnitName;
    type IntoIter = std::vec::IntoIter<UnitName>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a> IntoIterator for &'a Plugins {
    type Item = &'a UnitName;
    type IntoIter = std::slice::Iter<'a, UnitName>;

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
    Pattern(#[from] PatternError),
}
