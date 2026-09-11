use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};

use crate::fs::{Directory, TempPath};

#[cfg(test)]
mod tests;

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WriteStep {
    Write,
    FlushFile,
    Rename,
    FlushDirectory,
}

/// A file written by write-then-rename: readers see either the old contents
/// or the new ones, never a partial write.
///
/// Parent directories and their ancestors are flushed before publication;
/// file contents are flushed before rename and the containing directory after it.
#[derive(Debug, Clone)]
pub struct AtomicFile {
    path: PathBuf,
    #[cfg(test)]
    fault: Option<WriteStep>,
}

impl AtomicFile {
    pub fn at(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            #[cfg(test)]
            fault: None,
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Write `bytes`, creating parent directories. Failed writes attempt to
    /// remove the temporary file. A directory-sync failure after rename leaves
    /// the new contents visible, but their durability is uncertain.
    pub fn write(&self, bytes: &[u8]) -> io::Result<()> {
        let dir = match self.path.parent() {
            Some(parent) if !parent.as_os_str().is_empty() => parent,
            _ => Path::new("."),
        };
        Directory::create_all(dir)?;

        let tmp = TempPath::sibling(&self.path, "tmp");
        let file = File::options().write(true).create_new(true).open(&tmp)?;
        match self.replace(file, &tmp, bytes, dir) {
            Ok(()) => Ok(()),
            Err(e) => {
                let _ = std::fs::remove_file(&tmp);
                Err(e)
            }
        }
    }

    fn replace(&self, file: File, tmp: &Path, bytes: &[u8], dir: &Path) -> io::Result<()> {
        // The contents must be on disk before the rename can publish them.
        #[cfg(test)]
        self.check(WriteStep::Write)?;
        std::io::Write::write_all(&mut &file, bytes)?;
        #[cfg(test)]
        self.check(WriteStep::FlushFile)?;
        file.sync_all()?;
        drop(file);

        #[cfg(test)]
        self.check(WriteStep::Rename)?;
        std::fs::rename(tmp, &self.path)?;

        #[cfg(test)]
        self.check(WriteStep::FlushDirectory)?;
        Directory::sync(dir)
    }

    #[cfg(test)]
    fn check(&self, step: WriteStep) -> io::Result<()> {
        if self.fault == Some(step) {
            Err(io::Error::from_raw_os_error(libc::EIO))
        } else {
            Ok(())
        }
    }
}
