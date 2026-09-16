//! Source workspace roles; runtime units remain a separate concern.

use crate::{Layout, package::PackageName};
use std::path::{Path, PathBuf};

/// Every config Cargo member has one role, determined by its location.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkspaceRole {
    System,
    Plugin(PackageName),
    Library(PackageName),
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
            return Ok(Self::Plugin(PackageName::parse(name)?));
        }
        if directory.parent() == Some(layout.crates_dir().as_path()) {
            return Ok(Self::Library(PackageName::parse(name)?));
        }
        Err(WorkspaceError::Location(directory.into()))
    }

    /// An interrupted migration must be repaired before any build or scaffold.
    pub fn check_layout(layout: &Layout) -> Result<(), WorkspaceError> {
        if layout.migration_journal().try_exists()? {
            return Err(WorkspaceError::Interrupted);
        }
        match std::fs::symlink_metadata(layout.legacy_plugins_dir()) {
            Ok(_) => Err(WorkspaceError::Legacy),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.into()),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum WorkspaceError {
    #[error("legacy units/ directory found; run omega migrate --check, then omega migrate")]
    Legacy,
    #[error("workspace migration was interrupted; run omega migrate to recover")]
    Interrupted,
    #[error("{} is outside system/, plugins/<name>/, or crates/<name>/", .0.display())]
    Location(PathBuf),
    #[error(transparent)]
    Name(#[from] crate::package::PackageNameError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}
