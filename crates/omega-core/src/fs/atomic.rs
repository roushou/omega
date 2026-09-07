use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};

use crate::fs::TempPath;

/// A file written by write-then-rename: readers see either the old contents
/// or the new ones, never a partial write.
///
/// Both halves are flushed to the disk, not just to the page cache. Without
/// that, a crash seconds after a "successful" write can leave an empty file
/// where a manifest used to be — the rename is atomic with respect to other
/// readers, not with respect to power loss.
#[derive(Debug, Clone)]
pub struct AtomicFile {
    path: PathBuf,
}

impl AtomicFile {
    pub fn at(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Write `bytes`, creating parent directories. The temp file is removed
    /// if anything fails, so a failure leaves nothing behind.
    pub fn write(&self, bytes: &[u8]) -> io::Result<()> {
        let dir = match self.path.parent() {
            Some(parent) if !parent.as_os_str().is_empty() => parent,
            _ => Path::new("."),
        };
        std::fs::create_dir_all(dir)?;

        let tmp = TempPath::sibling(&self.path, "tmp");
        match self.replace(&tmp, bytes, dir) {
            Ok(()) => Ok(()),
            Err(e) => {
                let _ = std::fs::remove_file(&tmp);
                Err(e)
            }
        }
    }

    fn replace(&self, tmp: &Path, bytes: &[u8], dir: &Path) -> io::Result<()> {
        // The contents must be on disk before the rename can publish them.
        let file = File::create(tmp)?;
        std::io::Write::write_all(&mut &file, bytes)?;
        file.sync_all()?;
        drop(file);

        std::fs::rename(tmp, &self.path)?;

        // ...and the rename itself must be on disk, which is a property of
        // the directory, not the file.
        Self::sync_dir(dir)
    }

    /// Flush a directory entry. Not every filesystem supports it; one that
    /// does not was never going to lose the rename either.
    pub(crate) fn sync_dir(dir: &Path) -> io::Result<()> {
        match File::open(dir) {
            Ok(handle) => match handle.sync_all() {
                Err(e) if e.kind() == io::ErrorKind::InvalidInput => Ok(()),
                other => other,
            },
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e),
        }
    }
}
