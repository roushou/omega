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
    /// Discover conventional `plugins/` members and explicitly marked executable
    /// packages. Other Cargo members are libraries; `exclude` is respected.
    pub fn discover(layout: &Layout) -> Result<Self, PluginsError> {
        let manifest = layout.file::<Manifest>(CargoSlot::Workspace).read()?;
        let workspace = manifest.workspace()?.ok_or_else(|| CargoError {
            field: "workspace".into(),
            reason: "is required".into(),
        })?;
        let mut names = Vec::new();
        for dir in workspace.member_dirs(&layout.config)? {
            let role = match WorkspaceRole::at(layout, &dir) {
                Ok(role) => Some(role),
                Err(WorkspaceError::Location(_)) => None,
                Err(error) => return Err(error.into()),
            };
            if std::fs::symlink_metadata(&dir)
                .map_err(WorkspaceError::Io)?
                .is_symlink()
            {
                return Err(WorkspaceError::Location(dir).into());
            }
            if !dir
                .canonicalize()
                .map_err(WorkspaceError::Io)?
                .starts_with(layout.config.canonicalize().map_err(WorkspaceError::Io)?)
            {
                return Err(WorkspaceError::Location(dir).into());
            }
            let member = layout.file::<Manifest>(CargoSlot::Member(&dir)).read()?;
            if let Some(package) = member.package()? {
                match package.omega_kind()? {
                    Some("command-host") | Some("plugin") => {
                        names.push(package.name()?.parse().map_err(|error| CargoError {
                            field: "package.name".into(),
                            reason: format!("{error}"),
                        })?);
                        continue;
                    }
                    Some(other) => {
                        return Err(CargoError {
                            field: "package.metadata.omega.kind".into(),
                            reason: format!("unknown Omega role {other}"),
                        }
                        .into());
                    }
                    None => {}
                }
            }
            match role {
                Some(WorkspaceRole::Plugin(name)) => names.push(name.plugin().clone()),
                Some(
                    WorkspaceRole::System | WorkspaceRole::Library(_) | WorkspaceRole::Commands(_),
                ) => {}
                None => {}
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
