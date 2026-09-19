//! Source workspace roles; runtime plugins remain a separate concern.

use crate::{Layout, package::PackageName};
use std::path::{Path, PathBuf};

/// Every config Cargo member has one role, determined by its location.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkspaceRole {
    System,
    Plugin(PackageName),
    Library(PackageName),
    /// Command packages; location alone does not declare an executable.
    Commands(PackageName),
}

impl WorkspaceRole {
    pub fn at(layout: &Layout, directory: &Path) -> Result<Self, WorkspaceError> {
        if let Ok(metadata) = std::fs::symlink_metadata(directory)
            && metadata.is_symlink()
        {
            return Err(WorkspaceError::Location(directory.into()));
        }
        if directory == layout.system_dir() {
            return Ok(Self::System);
        }
        let name = directory.file_name().and_then(|s| s.to_str());
        let Some(name) = name else {
            return Err(WorkspaceError::Location(directory.into()));
        };
        if directory.parent() == Some(layout.plugins_dir().as_path()) {
            return Ok(Self::Plugin(name.parse::<PackageName>()?));
        }
        if directory.parent() == Some(layout.crates_dir().as_path()) {
            return Ok(Self::Library(name.parse::<PackageName>()?));
        }
        if directory.parent() == Some(layout.commands_dir().as_path()) {
            return Ok(Self::Commands(name.parse::<PackageName>()?));
        }
        Err(WorkspaceError::Location(directory.into()))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum WorkspaceError {
    #[error("{} is outside system/, plugins/<name>/, crates/<name>/, or commands/<name>/", .0.display())]
    Location(PathBuf),
    #[error(transparent)]
    Name(#[from] crate::package::PackageNameError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}
