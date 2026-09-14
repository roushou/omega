use anyhow::{Context, Result, ensure};
use omega_host::{AtomicFile, Directory};
use std::path::{Path, PathBuf};

/// Original bytes make stale preparation and safe rollback observable.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct FileEdit {
    path: PathBuf,
    before: Option<String>,
    after: String,
}

impl FileEdit {
    pub(crate) fn read(path: PathBuf) -> Result<Self> {
        let before = Self::contents(&path)?;
        Ok(Self {
            path,
            after: before.clone().unwrap_or_default(),
            before,
        })
    }

    fn contents(path: &Path) -> Result<Option<String>> {
        match std::fs::symlink_metadata(path) {
            Ok(metadata) => ensure!(
                metadata.is_file(),
                "{} must be a regular file",
                path.display()
            ),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error).with_context(|| format!("reading {}", path.display())),
        }
        Ok(Some(
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?,
        ))
    }

    pub(super) fn path(&self) -> &Path {
        &self.path
    }

    pub(crate) fn exists(&self) -> bool {
        self.before.is_some()
    }
    pub(crate) fn source(&self) -> &str {
        &self.after
    }
    pub(crate) fn replace(&mut self, source: String) {
        self.after = source;
    }
    pub(crate) fn changed(&self) -> bool {
        self.before.as_deref() != Some(&self.after)
    }

    pub(super) fn check(&self) -> Result<()> {
        ensure!(
            Self::contents(&self.path)? == self.before,
            "{} changed during preparation; retry the command",
            self.path.display()
        );
        Ok(())
    }

    fn write(&self) -> Result<()> {
        self.check()?;
        if self.changed() {
            AtomicFile::at(&self.path)
                .write(self.after.as_bytes())
                .with_context(|| format!("writing {}", self.path.display()))?;
        }
        Ok(())
    }

    fn restore(&self) -> Result<()> {
        if !self.changed() {
            return Ok(());
        }
        let current = Self::contents(&self.path)?;
        if current == self.before {
            return Ok(());
        }
        ensure!(
            current.as_deref() == Some(&self.after),
            "{} changed externally; restore it manually",
            self.path.display()
        );
        match &self.before {
            Some(bytes) => AtomicFile::at(&self.path).write(bytes.as_bytes())?,
            None => {
                std::fs::remove_file(&self.path)?;
                Directory::sync(self.path.parent().context("file has no parent")?)?;
            }
        }
        Ok(())
    }
}

/// The file edits belonging to one workspace operation, in publication order.
#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
pub(crate) struct FileEdits(Vec<FileEdit>);

impl FileEdits {
    pub(crate) fn new(edits: Vec<FileEdit>) -> Self {
        Self(edits)
    }

    pub(crate) fn apply(&self) -> Result<()> {
        for edit in &self.0 {
            edit.check()?;
        }
        for (index, edit) in self.0.iter().enumerate() {
            if let Err(error) = edit.write() {
                return Err(Self::recover(&self.0[..=index], error));
            }
        }
        Ok(())
    }

    pub(super) fn iter(&self) -> impl Iterator<Item = &FileEdit> {
        self.0.iter()
    }

    pub(super) fn restore_all(&self) -> Result<()> {
        for edit in self.0.iter().rev() {
            edit.restore()?;
        }
        Ok(())
    }

    pub(crate) fn rollback(&self, error: anyhow::Error) -> anyhow::Error {
        Self::recover(&self.0, error)
    }

    fn recover(edits: &[FileEdit], error: anyhow::Error) -> anyhow::Error {
        let mut failures = Vec::new();
        for edit in edits.iter().rev() {
            if let Err(error) = edit.restore() {
                failures.push(format!("{}: {error:#}", edit.path.display()));
            }
        }
        if failures.is_empty() {
            error.context("workspace changes rolled back")
        } else {
            anyhow::anyhow!("{error:#}; incomplete rollback:\n{}", failures.join("\n"))
        }
    }
}
