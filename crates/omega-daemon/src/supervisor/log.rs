//! Where a unit's output goes.
//!
//! A unit's stdout and stderr are handed straight to a file, so a panic
//! survives the restart that follows it. Without this, the one message that
//! explains a crash loop is interleaved into the daemon's own stderr and gone
//! by the time anyone looks.

use std::fs::{File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};
use std::process::Stdio;

#[derive(Debug, Clone)]
pub struct UnitLog {
    path: PathBuf,
}

impl UnitLog {
    /// How large a unit's log may grow before a restart starts it over. A
    /// crash-looping unit writes the same message forever; it should not fill
    /// the disk with it.
    pub const MAX_BYTES: u64 = 1024 * 1024;

    pub fn at(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Open the log for a run that is about to start, appending to what the
    /// previous runs left unless it has grown past the cap.
    pub fn open(&self) -> io::Result<File> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let oversized = std::fs::metadata(&self.path)
            .map(|meta| meta.len() > Self::MAX_BYTES)
            .unwrap_or(false);

        OpenOptions::new()
            .create(true)
            .write(true)
            .append(!oversized)
            .truncate(oversized)
            .open(&self.path)
    }

    /// Both output streams of a child, pointed at this log. The kernel writes
    /// them; the daemon never sits between a unit and its own output.
    pub fn streams(&self) -> io::Result<(Stdio, Stdio)> {
        let out = self.open()?;
        let err = out.try_clone()?;
        Ok((Stdio::from(out), Stdio::from(err)))
    }
}
