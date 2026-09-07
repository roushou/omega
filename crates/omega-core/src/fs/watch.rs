//! Watching for changes, rather than asking whether anything changed.
//!
//! Polling a config tree means walking every file on a timer to learn what
//! the kernel already knew. This is the kernel telling us instead — and the
//! difference shows up as a rebuild that starts when you save the file.
//!
//! Two things make a naive watch wrong here. A build writes many files, so
//! events are settled before they are reported: one save, one rebuild. And
//! `omega build` replaces whole directories by renaming them into place,
//! which destroys any watch on the directory itself — so a caller watches the
//! parent too, and the watch survives what it is watching for.

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
    /// Directories a config tree contains but does not consist of. A build
    /// writing into `target/` is not a change to the config, and treating it
    /// as one is how a watcher ends up rebuilding forever.
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
        // One slot: a full channel already means "something changed", which
        // is the whole message.
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

    /// Whether an event changed anything.
    ///
    /// Reading a file is an event too, and a rebuild reads the whole config —
    /// so treating opens as changes makes a watcher rebuild because it built,
    /// forever. Only what alters the tree counts.
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

/// Re-exported so callers say what they mean without depending on `notify`.
pub use notify::RecursiveMode as Recursion;
