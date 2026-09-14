//! Settled filesystem change notifications.
//! Ignore read events and build-output directories to prevent rebuild loops.
//! Watch parent directories when targets can be replaced by atomic rename.

use std::path::{Path, PathBuf};
use std::time::Duration;

use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use tokio::sync::mpsc;

#[derive(Debug, thiserror::Error)]
pub enum WatchError {
    #[error("cannot watch {}: {source}", path.display())]
    Watch {
        path: PathBuf,
        #[source]
        source: notify::Error,
    },
}

/// A stream of settled filesystem changes.
pub struct Changes {
    events: mpsc::Receiver<()>,
    settle: Duration,
    /// Dropping this stops the watch, so it is held for as long as changes
    /// are wanted.
    _watcher: RecommendedWatcher,
}

impl std::fmt::Debug for Changes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Changes")
            .field("settle", &self.settle)
            .finish()
    }
}

impl Changes {
    /// Ignore build outputs and metadata that must not trigger config rebuilds.
    pub const IGNORED: &'static [&'static str] = &["target", ".git", ".jj", "node_modules"];

    /// How long to wait for changes to stop arriving before reporting them.
    pub const SETTLE: Duration = Duration::from_millis(150);

    /// Watch `paths`, reporting a change once the writes stop.
    pub fn watch(paths: &[&Path], recursive: RecursiveMode) -> Result<Self, WatchError> {
        Self::with_settle(paths, recursive, Self::SETTLE)
    }

    pub fn with_settle(
        paths: &[&Path],
        recursive: RecursiveMode,
        settle: Duration,
    ) -> Result<Self, WatchError> {
        // Coalesce pending notifications into one wakeup.
        let (changed, events) = mpsc::channel(1);

        let mut watcher = notify::recommended_watcher(move |event: notify::Result<Event>| {
            let Ok(event) = event else {
                return;
            };
            tracing::debug!(kind = ?event.kind, paths = ?event.paths, "fs event");
            if !Self::is_change(&event.kind) {
                return;
            }
            if event.paths.iter().any(|path| Self::is_ignored(path)) {
                return;
            }
            let _ = changed.try_send(());
        })
        .map_err(|source| WatchError::Watch {
            path: PathBuf::new(),
            source,
        })?;

        for path in paths {
            watcher
                .watch(path, recursive)
                .map_err(|source| WatchError::Watch {
                    path: path.to_path_buf(),
                    source,
                })?;
        }

        Ok(Self {
            events,
            settle,
            _watcher: watcher,
        })
    }

    /// The next settled change. `None` when the watch has ended.
    pub async fn next(&mut self) -> Option<()> {
        self.events.recv().await?;

        // Wait out the rest of the burst: a build is thousands of writes and
        // one change.
        loop {
            match tokio::time::timeout(self.settle, self.events.recv()).await {
                Ok(Some(())) => continue,
                Ok(None) | Err(_) => return Some(()),
            }
        }
    }

    /// Accept only events that mutate the filesystem; reads must not trigger rebuilds.
    fn is_change(kind: &EventKind) -> bool {
        match kind {
            EventKind::Create(_) | EventKind::Remove(_) | EventKind::Modify(_) => true,
            EventKind::Access(_) | EventKind::Any | EventKind::Other => false,
        }
    }

    fn is_ignored(path: &Path) -> bool {
        path.components().any(|component| {
            component
                .as_os_str()
                .to_str()
                .is_some_and(|name| Self::IGNORED.contains(&name))
        })
    }
}

/// Watch recursion mode for callers.
pub use notify::RecursiveMode as Recursion;
