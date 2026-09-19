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

/// Whether a failed atomic replacement may already be visible at its destination.
#[derive(Debug, thiserror::Error)]
pub enum WriteError {
    #[error("file was not published: {0}")]
    BeforePublication(#[source] io::Error),
    #[error("file was published but directory sync failed: {0}")]
    AfterPublication(#[source] io::Error),
}

impl WriteError {
    pub fn into_io(self) -> io::Error {
        match self {
            Self::BeforePublication(error) | Self::AfterPublication(error) => error,
        }
    }
}

impl From<io::Error> for WriteError {
    fn from(error: io::Error) -> Self {
        Self::BeforePublication(error)
    }
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
        self.write_mode(bytes, None).map_err(WriteError::into_io)
    }

    /// Publish contents and permissions together; permission bits are set on the
    /// temporary file before it is flushed and renamed over the destination.
    pub fn write_with_permissions(
        &self,
        bytes: &[u8],
        permissions: std::fs::Permissions,
    ) -> io::Result<()> {
        self.publish(bytes, permissions)
            .map_err(WriteError::into_io)
    }

    /// Publish with an error that distinguishes rejection before rename from
    /// uncertain durability after rename. Success includes file and directory sync.
    pub fn publish(
        &self,
        bytes: &[u8],
        permissions: std::fs::Permissions,
    ) -> Result<(), WriteError> {
        self.write_mode(bytes, Some(permissions))
    }

    fn write_mode(
        &self,
        bytes: &[u8],
        permissions: Option<std::fs::Permissions>,
    ) -> Result<(), WriteError> {
        let dir = match self.path.parent() {
            Some(parent) if !parent.as_os_str().is_empty() => parent,
            _ => Path::new("."),
        };
        Directory::create_all(dir)?;

        let tmp = TempPath::sibling(&self.path, "tmp");
        let file = File::options().write(true).create_new(true).open(&tmp)?;
        let result = (|| {
            if let Some(permissions) = permissions {
                file.set_permissions(permissions)?;
            }
            self.replace(file, &tmp, bytes, dir)
        })();
        match result {
            Ok(()) => Ok(()),
            Err(e) => {
                let _ = std::fs::remove_file(&tmp);
                Err(e)
            }
        }
    }

    fn replace(&self, file: File, tmp: &Path, bytes: &[u8], dir: &Path) -> Result<(), WriteError> {
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
        self.check(WriteStep::FlushDirectory)
            .map_err(WriteError::AfterPublication)?;
        Directory::sync(dir).map_err(WriteError::AfterPublication)
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
