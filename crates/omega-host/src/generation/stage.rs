use super::Generations;
use crate::{AtomicFile, Directory, Layout, StageDir};
use std::{io, path::Path};

#[cfg(test)]
mod tests;

#[derive(Debug)]
pub struct GenerationStage {
    pub(super) stage: Option<StageDir>,
    pub(super) target: std::path::PathBuf,
    pub(super) layout: Layout,
    #[cfg(test)]
    pub(super) checkpoint: Option<fn(&str)>,
}

impl GenerationStage {
    pub fn files(&self) -> &StageDir {
        self.stage
            .as_ref()
            .expect("a stage is available until commit")
    }

    pub fn commit(mut self) -> io::Result<Layout> {
        let store = Generations::new(&self.layout);
        let _transaction = store.transaction()?;
        let staged_layout =
            Layout::at(&self.layout.config, self.files().path(), &self.layout.cache);
        Self::sync_tree(self.files().path())?;
        AtomicFile::at(staged_layout.generation_ready()).write(b"1\n")?;
        #[cfg(test)]
        self.checkpoint("prepared");
        self.stage
            .take()
            .expect("a stage is committed once")
            .commit_new()?;
        Directory::sync(&self.layout.state)?;
        #[cfg(test)]
        self.checkpoint("published");
        let name = self
            .target
            .file_name()
            .expect("a generation has a directory name")
            .to_str()
            .expect("generated names are ASCII");
        AtomicFile::at(self.layout.active_build()).write(format!("{name}\n").as_bytes())?;
        #[cfg(test)]
        self.checkpoint("activated");
        Ok(Layout::at(
            &self.layout.config,
            &self.target,
            &self.layout.cache,
        ))
    }

    #[cfg(test)]
    fn checkpoint(&self, point: &str) {
        if let Some(checkpoint) = self.checkpoint {
            checkpoint(point);
        }
    }

    fn sync_tree(path: &Path) -> io::Result<()> {
        for entry in std::fs::read_dir(path)? {
            let entry = entry?;
            let kind = entry.file_type()?;
            if kind.is_dir() {
                Self::sync_tree(&entry.path())?;
            } else if kind.is_file() {
                std::fs::File::open(entry.path())?.sync_all()?;
            } else {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "build generations contain only files and directories",
                ));
            }
        }
        Directory::sync(path)
    }
}

impl Drop for GenerationStage {
    fn drop(&mut self) {
        // Only an empty reservation can be removed; a published generation
        // contains its readiness marker, including after a failed parent flush.
        let _ = std::fs::remove_dir(&self.target);
    }
}
