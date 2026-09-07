use std::io;
use std::path::{Path, PathBuf};

use omega_proto::toml::{TomlFile, TomlSchema};
use omega_proto::{AtomicFile, TempPath};

/// A directory built off to the side that replaces its target in one rename.
///
/// A build writes everything into the stage, then [`commit`](Self::commit)
/// swaps it in. A failed build drops the stage (cleaned up in `Drop`) and the
/// live directory is never touched.
#[derive(Debug)]
pub struct StageDir {
    final_dir: PathBuf,
    staging: PathBuf,
    committed: bool,
}

impl StageDir {
    /// Create a fresh stage for `final_dir`, clearing any leftover stage from
    /// a previous crashed build.
    pub fn new(final_dir: &Path) -> io::Result<Self> {
        let staging = TempPath::sibling(final_dir, "stage");
        let _ = std::fs::remove_dir_all(&staging);
        std::fs::create_dir_all(&staging)?;
        Ok(Self {
            final_dir: final_dir.to_path_buf(),
            staging,
            committed: false,
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
        Self::parents(&dest)?;
        std::fs::write(dest, bytes)
    }

    /// Copy `src` to `rel` inside the stage, creating parents.
    pub fn copy(&self, src: &Path, rel: impl AsRef<Path>) -> io::Result<()> {
        let dest = self.staging.join(rel);
        Self::parents(&dest)?;
        std::fs::copy(src, dest)?;
        Ok(())
    }

    /// Swap the stage into place, consuming it. The previous directory is
    /// moved aside first and restored if the swap fails.
    pub fn commit(mut self) -> io::Result<()> {
        let backup = TempPath::sibling(&self.final_dir, "old");
        let _ = std::fs::remove_dir_all(&backup);

        let had_old = self.final_dir.exists();
        if had_old {
            std::fs::rename(&self.final_dir, &backup)?;
        }
        if let Err(e) = std::fs::rename(&self.staging, &self.final_dir) {
            if had_old {
                let _ = std::fs::rename(&backup, &self.final_dir);
            }
            return Err(e);
        }

        self.committed = true;
        if had_old {
            let _ = std::fs::remove_dir_all(&backup);
        }

        // The swap is only durable once the directory entry is.
        if let Some(parent) = self.final_dir.parent() {
            AtomicFile::sync_dir(parent)?;
        }
        Ok(())
    }

    fn parents(dest: &Path) -> io::Result<()> {
        match dest.parent() {
            Some(parent) => std::fs::create_dir_all(parent),
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
