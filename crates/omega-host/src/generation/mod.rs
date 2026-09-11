//! Durable recovery references and leases for immutable build generations.

mod history;
mod id;
mod stage;

use crate::fs::FileLock;
use crate::{AtomicFile, Directory, Layout, StageDir, TempPath};
use history::History;
use std::{collections::BTreeSet, io, sync::Arc};

pub use id::GenerationId;
pub use stage::GenerationStage;

/// A generation whose directory cannot be reclaimed while any clone lives.
#[derive(Debug, Clone)]
pub struct Generation {
    id: GenerationId,
    layout: Layout,
    root: Layout,
    _lease: Arc<FileLock>,
}

impl Generation {
    pub fn id(&self) -> &GenerationId {
        &self.id
    }
    pub fn layout(&self) -> &Layout {
        &self.layout
    }

    /// Keep this generation leased by the child, even if its supervisor crashes.
    /// Descendants inherit the lease too; it closes when their last descriptor closes.
    pub fn protect_child(&self, command: &mut std::process::Command) {
        self._lease.protect_child(command);
    }

    /// Persist acceptance before exposing the new manifests and settings.
    /// Acceptance means validation and handover preparation succeeded, not a health check.
    pub fn accept(&self) -> io::Result<()> {
        let store = Generations::new(&self.root);
        let _transaction = store.transaction()?;
        let mut history = store.history()?;
        if history.accepted.as_ref() != Some(&self.id) {
            history.previous = history.accepted;
            history.accepted = Some(self.id.clone());
            self.root
                .file::<History>(())
                .write(&history)
                .map_err(io::Error::other)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct Generations {
    layout: Layout,
}

impl Generations {
    pub fn new(layout: &Layout) -> Self {
        Self {
            layout: layout.clone(),
        }
    }

    pub fn pin_current(&self) -> io::Result<Option<Generation>> {
        let _transaction = self.transaction()?;
        match self.current_id()? {
            Some(id) => self.pin_locked(id).map(Some),
            None if self.layout.state_units_toml().exists() => Err(io::Error::other(
                "legacy build layout; run `omega build` to publish a generation",
            )),
            None => Ok(None),
        }
    }

    pub fn pin(&self, id: &GenerationId) -> io::Result<Generation> {
        let _transaction = self.transaction()?;
        self.pin_locked(id.clone())
    }

    /// Try these in order, validating each before accepting it.
    pub fn recovery_ids(&self) -> io::Result<Vec<GenerationId>> {
        let _transaction = self.transaction()?;
        let history = self.history()?;
        Ok(history
            .accepted
            .into_iter()
            .chain(history.previous)
            .collect())
    }

    /// Reserve a rollback while the caller validates its target.
    /// Publication and cleanup wait until this reservation is committed or dropped.
    /// The caller must not acquire another store transaction while holding it.
    pub fn rollback(&self, target: Option<&GenerationId>) -> io::Result<Rollback> {
        let transaction = self.transaction()?;
        let history = self.history()?;
        let id = match target {
            Some(id) => id.clone(),
            None => {
                // An invalid current reference is itself a reason to restore acceptance.
                let current = match self.current_id() {
                    Ok(current) => current,
                    Err(error) if error.kind() == io::ErrorKind::InvalidData => None,
                    Err(error) => return Err(error),
                };
                if history.accepted != current {
                    history.accepted
                } else {
                    history.previous
                }
                .ok_or_else(|| {
                    io::Error::other("no accepted generation is available for rollback")
                })?
            }
        };
        let generation = self.pin_locked(id)?;
        Ok(Rollback {
            generation,
            _transaction: transaction,
        })
    }

    /// Remove only complete managed generations with neither a reference nor a lease.
    /// Broken references abort the operation before any directory is removed.
    pub fn clean(&self) -> io::Result<Vec<GenerationId>> {
        let _transaction = self.transaction()?;
        let history = self.history()?;
        let protected: BTreeSet<_> = self
            .current_id()?
            .into_iter()
            .chain(history.accepted)
            .chain(history.previous)
            .collect();
        let entries = match std::fs::read_dir(self.layout.generations_dir()) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error),
        };
        let mut candidates = Vec::new();
        for entry in entries {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            let id = GenerationId::parse(name)?;
            if protected.contains(&id) {
                continue;
            }
            let layout = self.layout_of(&id);
            // Unmarked directories include private stages and builds predating leases.
            if !Self::ready(&layout)? {
                continue;
            }
            candidates.push(id);
        }
        candidates.sort();
        let mut removed = Vec::new();
        for id in candidates {
            let layout = self.layout_of(&id);
            let Some(_lease) = FileLock::try_exclusive(&layout.generation_lease())? else {
                continue;
            };
            std::fs::remove_dir_all(&layout.state)?;
            removed.push(id);
        }
        Directory::sync(&self.layout.generations_dir())?;
        Ok(removed)
    }

    fn current_id(&self) -> io::Result<Option<GenerationId>> {
        match std::fs::read_to_string(self.layout.active_build()) {
            Ok(reference) => GenerationId::parse(reference.trim_end_matches('\n')).map(Some),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error),
        }
    }

    fn history(&self) -> io::Result<History> {
        self.layout
            .file::<History>(())
            .read_or_default()
            .map_err(io::Error::other)
    }

    fn layout_of(&self, id: &GenerationId) -> Layout {
        Layout::at(
            &self.layout.config,
            self.layout.generations_dir().join(id.as_str()),
            &self.layout.cache,
        )
    }

    fn ready(layout: &Layout) -> io::Result<bool> {
        match std::fs::symlink_metadata(layout.generation_ready()) {
            Ok(metadata) => Ok(metadata.is_file()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(error),
        }
    }

    fn pin_locked(&self, id: GenerationId) -> io::Result<Generation> {
        let layout = self.layout_of(&id);
        if !std::fs::symlink_metadata(&layout.state)?
            .file_type()
            .is_dir()
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "generation is not a directory",
            ));
        }
        if !Self::ready(&layout)? {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "generation is incomplete: missing regular readiness marker",
            ));
        }
        let lease = FileLock::shared(&layout.generation_lease())?;
        Ok(Generation {
            id,
            layout,
            root: self.layout.clone(),
            _lease: Arc::new(lease),
        })
    }

    fn transaction(&self) -> io::Result<FileLock> {
        // Lock files need not survive a reboot; publication flushes the ancestry.
        std::fs::create_dir_all(&self.layout.state)?;
        FileLock::exclusive(&self.layout.generation_lock())
    }
    /// A private stage. Dropping it cannot alter the active build.
    pub fn stage(&self) -> io::Result<GenerationStage> {
        let _transaction = self.transaction()?;
        std::fs::create_dir_all(self.layout.generations_dir())?;
        let target = TempPath::sibling(&self.layout.generations_dir().join("build"), "generation");
        // Reserve the destination so concurrent publishers cannot share it.
        std::fs::create_dir(&target)?;
        let stage = match StageDir::new(&target) {
            Ok(stage) => stage,
            Err(error) => {
                let _ = std::fs::remove_dir(&target);
                return Err(error);
            }
        };
        Ok(GenerationStage {
            stage: Some(stage),
            target,
            layout: self.layout.clone(),
            #[cfg(test)]
            checkpoint: None,
        })
    }
}

/// A validated target can be published without a race against a concurrent build.
#[derive(Debug)]
pub struct Rollback {
    generation: Generation,
    _transaction: FileLock,
}
impl Rollback {
    pub fn generation(&self) -> &Generation {
        &self.generation
    }
    pub fn commit(self) -> io::Result<Generation> {
        AtomicFile::at(self.generation.root.active_build())
            .write(format!("{}\n", self.generation.id).as_bytes())?;
        Ok(self.generation)
    }
}
