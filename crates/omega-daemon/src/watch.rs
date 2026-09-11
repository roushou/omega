//! Detect publication of an immutable build generation.

use std::path::Path;

use crate::host::{Changes, Recursion, WatchError};
use omega_host::Layout;

/// The active reference contents, independent of filesystem timestamp precision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateStamp {
    reference: Option<Vec<u8>>,
}

impl StateStamp {
    /// Watch reference replacement and creation of a previously absent state root.
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
            reference: std::fs::read(layout.active_build()).ok(),
        }
    }

    /// Whether the state dir has been rebuilt since this stamp was taken.
    pub fn changed(&self, layout: &Layout) -> bool {
        Self::of(layout) != *self
    }
}
