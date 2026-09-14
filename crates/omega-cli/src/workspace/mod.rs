mod editor;
mod files;
mod init;
mod migration;
mod plugin;

pub(crate) use crate::scaffold::PluginName;
pub(crate) use editor::CargoEditor;
pub(crate) use files::{FileEdit, FileEdits};
pub(crate) use init::InitialShell;

use crate::scaffold::Scaffold;
use anyhow::Result;
use omega_host::fs::FileLock;
use omega_host::{Directory, Layout};

/// Holds the workspace mutation lock from inspection through publication.
#[derive(Debug)]
pub(crate) struct ConfigWorkspace {
    layout: Layout,
    scaffold: Scaffold,
    _lock: FileLock,
}

impl ConfigWorkspace {
    pub(crate) fn open(layout: Layout) -> Result<Self> {
        let path = layout.workspace_lock();
        Directory::create_all(path.parent().expect("workspace lock has a parent"))?;
        let lock = FileLock::exclusive(&path)?;
        Ok(Self {
            layout,
            scaffold: Scaffold::new(),
            _lock: lock,
        })
    }

    pub(crate) fn layout(&self) -> &Layout {
        &self.layout
    }
}

#[cfg(test)]
mod tests;
