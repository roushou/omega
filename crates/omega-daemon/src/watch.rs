//! Noticing that the built state changed.
//!
//! `omega build` swaps the whole state dir into place in one rename, so the
//! two files that describe it — what was built, and what it is for — change
//! together or not at all. Watching them is how a running daemon learns that
//! the config was rebuilt, without being restarted.

use std::path::Path;
use std::time::SystemTime;

use crate::host::{Changes, Recursion, WatchError};
use omega_document::DocumentFile;
use omega_host::Layout;

/// A cheap stamp of the built state: the two files' modification times.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateStamp {
    document: Option<SystemTime>,
    units: Option<SystemTime>,
}

impl StateStamp {
    /// Watch for a rebuild: the state dir for writes into it, and its parent
    /// for the rename that replaces it wholesale.
    pub fn watch(layout: &Layout, settle: std::time::Duration) -> Result<Changes, WatchError> {
        let parent = layout.state.parent().unwrap_or(&layout.state);
        let watched: Vec<&Path> = if layout.state.exists() {
            vec![layout.state.as_path(), parent]
        } else {
            vec![parent]
        };
        Changes::with_settle(&watched, Recursion::NonRecursive, settle)
    }

    pub fn of(layout: &Layout) -> Self {
        Self {
            document: Self::mtime(&layout.state.join(DocumentFile::FILE_NAME)),
            units: Self::mtime(&layout.state_units_toml()),
        }
    }

    /// Whether the state dir has been rebuilt since this stamp was taken.
    pub fn changed(&self, layout: &Layout) -> bool {
        Self::of(layout) != *self
    }

    fn mtime(path: &Path) -> Option<SystemTime> {
        std::fs::metadata(path)
            .and_then(|meta| meta.modified())
            .ok()
    }
}
