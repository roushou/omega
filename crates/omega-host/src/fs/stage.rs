use std::ffi::CString;
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use crate::toml::{TomlFile, TomlSchema};
use crate::{AtomicFile, Directory, TempPath};

/// A directory built off to the side before publication.
///
/// A build writes everything into the stage, then [`commit`](Self::commit)
/// swaps it in. Dropping an uncommitted stage removes its files.
#[derive(Debug)]
pub struct StageDir {
    final_dir: PathBuf,
    staging: PathBuf,
    committed: bool,
    #[cfg(test)]
    checkpoint: Option<fn(&str) -> io::Result<()>>,
}

impl StageDir {
    /// Reserve a fresh stage for `final_dir`. Existing directories are never reused.
    pub fn new(final_dir: &Path) -> io::Result<Self> {
        let staging = TempPath::sibling(final_dir, "stage");
        if let Some(parent) = staging
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            Directory::create_all(parent)?;
        }
        std::fs::create_dir(&staging)?;
        Ok(Self {
            final_dir: final_dir.to_path_buf(),
            staging,
            committed: false,
            #[cfg(test)]
            checkpoint: None,
        })
    }

    pub fn path(&self) -> &Path {
        &self.staging
    }

    /// A document inside the stage, addressed the same way as any other:
    /// staged writes go through [`TomlFile`], not a second writer.
    pub fn file<S: TomlSchema>(&self, rel: impl AsRef<Path>) -> TomlFile<S> {
        TomlFile::at(self.staging.join(rel))
    }

    /// Write raw bytes to `rel` inside the stage, creating parents.
    pub fn write(&self, rel: impl AsRef<Path>, bytes: &[u8]) -> io::Result<()> {
        let dest = self.staging.join(rel);
        AtomicFile::at(dest).write(bytes)
    }

    /// Copy `src` to `rel` inside the stage, creating parents.
    pub fn copy(&self, src: &Path, rel: impl AsRef<Path>) -> io::Result<()> {
        let dest = self.staging.join(rel);
        Self::parents(&dest)?;
        std::fs::copy(src, &dest)?;
        std::fs::File::open(&dest)?.sync_all()?;
        if let Some(parent) = dest.parent() {
            Directory::sync(parent)?;
        }
        Ok(())
    }

    /// Publish over an empty reservation in one rename. A nonempty destination
    /// is refused by the filesystem and is never moved aside.
    pub(crate) fn commit_new(mut self) -> io::Result<()> {
        std::fs::rename(&self.staging, &self.final_dir)?;
        self.committed = true;
        if let Some(parent) = self.final_dir.parent() {
            Directory::sync(parent)?;
        }
        Ok(())
    }

    /// Publish atomically, exchanging an existing entry with the stage.
    ///
    /// The filesystem must support Linux atomic rename flags. An error after
    /// publication can leave the new directory visible; retrying is safe. The
    /// displaced entry is removed only after publication has been flushed.
    pub fn commit(mut self) -> io::Result<()> {
        Directory::sync(&self.staging)?;
        #[cfg(test)]
        self.checkpoint("prepared")?;
        let replaced = match self.rename(libc::RENAME_EXCHANGE) {
            Ok(()) => true,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                // A concurrent publisher must not be overwritten by the first-install path.
                self.rename(libc::RENAME_NOREPLACE)?;
                false
            }
            Err(error) => return Err(error),
        };
        // After exchange, staging holds the displaced entry. Drop must not
        // delete it if publication's directory flush fails.
        self.committed = true;
        #[cfg(test)]
        self.checkpoint("published")?;
        Self::sync_parent(&self.final_dir)?;
        #[cfg(test)]
        self.checkpoint("durable")?;
        if replaced {
            let metadata = std::fs::symlink_metadata(&self.staging)?;
            if metadata.is_dir() {
                std::fs::remove_dir_all(&self.staging)?;
            } else {
                std::fs::remove_file(&self.staging)?;
            }
            Self::sync_parent(&self.final_dir)?;
        }
        Ok(())
    }

    fn rename(&self, flags: libc::c_uint) -> io::Result<()> {
        let source = CString::new(self.staging.as_os_str().as_bytes())
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
        let destination = CString::new(self.final_dir.as_os_str().as_bytes())
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
        // Both C strings stay alive through the call; flags select one atomic operation.
        let result = unsafe {
            libc::renameat2(
                libc::AT_FDCWD,
                source.as_ptr(),
                libc::AT_FDCWD,
                destination.as_ptr(),
                flags,
            )
        };
        if result == 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }

    fn sync_parent(path: &Path) -> io::Result<()> {
        Directory::sync(path.parent().unwrap_or(Path::new(".")))
    }

    #[cfg(test)]
    fn checkpoint(&self, point: &str) -> io::Result<()> {
        match self.checkpoint {
            Some(checkpoint) => checkpoint(point),
            None => Ok(()),
        }
    }

    fn parents(dest: &Path) -> io::Result<()> {
        match dest.parent() {
            Some(parent) => Directory::create_all(parent),
            None => Ok(()),
        }
    }
}

impl Drop for StageDir {
    fn drop(&mut self) {
        if !self.committed {
            let _ = std::fs::remove_dir_all(&self.staging);
        }
    }
}

#[cfg(test)]
mod tests;
