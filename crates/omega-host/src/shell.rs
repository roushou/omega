//! Transactional installation of generated shell configuration.
use crate::{AtomicFile, GenerationId, Layout};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{io, path::Path};

#[derive(Debug, thiserror::Error)]
pub enum InstallError {
    #[error(
        "shell configuration has external changes; inspect with omega shell diff, then apply --overwrite or adopt the changes"
    )]
    Conflict,
    #[error("shell configuration is not managed; run omega shell adopt first")]
    Unmanaged,
    #[error("shell configuration changed while preparing the write; retry")]
    Changed,
    #[error("unsupported shell configuration: {0}")]
    Invalid(String),
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallationState {
    Unmanaged,
    Current,
    ModifiedExternally,
    Missing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApplyOutcome {
    Unchanged,
    Installed,
}

/// Persist the last configuration itself to compare semantic content without hash collisions.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Receipt {
    generation: Option<GenerationId>,
    installed: Value,
    /// Written before replacing the target so interruption between writes is recoverable.
    pending: Option<Value>,
    target: std::path::PathBuf,
}

#[derive(Debug)]
pub struct ShellInstallation {
    layout: Layout,
}
impl ShellInstallation {
    pub fn new(layout: &Layout) -> Self {
        Self {
            layout: layout.clone(),
        }
    }

    pub fn read(&self) -> Result<Option<Value>, InstallError> {
        Self::read_json(&self.layout.shell_config)
    }
    fn read_json(path: &Path) -> Result<Option<Value>, InstallError> {
        match std::fs::read(path) {
            Ok(bytes) => Ok(Some(serde_json::from_slice(&bytes)?)),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.into()),
        }
    }
    fn receipt(&self) -> Result<Option<Receipt>, InstallError> {
        Self::read_json(&self.layout.shell_receipt())?
            .map(serde_json::from_value)
            .transpose()
            .map_err(Into::into)
    }
    fn state(&self, current: Option<&Value>, receipt: Option<&Receipt>) -> InstallationState {
        let Some(receipt) = receipt.filter(|receipt| receipt.target == self.layout.shell_config)
        else {
            return InstallationState::Unmanaged;
        };
        match current {
            None => InstallationState::Missing,
            Some(current)
                if current == &receipt.installed || receipt.pending.as_ref() == Some(current) =>
            {
                InstallationState::Current
            }
            Some(_) => InstallationState::ModifiedExternally,
        }
    }
    pub fn inspect(&self) -> Result<InstallationState, InstallError> {
        Ok(self.state(self.read()?.as_ref(), self.receipt()?.as_ref()))
    }
    fn lock(&self) -> Result<crate::fs::FileLock, InstallError> {
        std::fs::create_dir_all(self.layout.generations_dir())?;
        Ok(crate::fs::FileLock::exclusive(&self.layout.shell_lock())?)
    }
    fn save_receipt(&self, receipt: &Receipt) -> Result<(), InstallError> {
        AtomicFile::at(self.layout.shell_receipt()).write(&serde_json::to_vec_pretty(receipt)?)?;
        Ok(())
    }

    /// Establish ownership without changing the desktop. Re-adoption preserves the first backup.
    pub fn adopt(&self, expected: &Value) -> Result<(), InstallError> {
        let _lock = self.lock()?;
        let original = std::fs::read(&self.layout.shell_config)?;
        let current: Value = serde_json::from_slice(&original)?;
        if &current != expected {
            return Err(InstallError::Changed);
        }
        Self::validate(&current)?;
        if !self.layout.shell_backup().exists() {
            AtomicFile::at(self.layout.shell_backup()).write(&original)?;
        }
        self.save_receipt(&Receipt {
            generation: None,
            installed: current,
            pending: None,
            target: self.layout.shell_config.clone(),
        })
    }

    pub fn apply(
        &self,
        desired: &Value,
        generation: &GenerationId,
        overwrite: bool,
    ) -> Result<ApplyOutcome, InstallError> {
        Self::validate(desired)?;
        let _lock = self.lock()?;
        let before = std::fs::read(&self.layout.shell_config).or_else(|error| {
            if error.kind() == io::ErrorKind::NotFound {
                Ok(Vec::new())
            } else {
                Err(error)
            }
        })?;
        let current = if before.is_empty() {
            None
        } else {
            Some(serde_json::from_slice::<Value>(&before)?)
        };
        let receipt = self.receipt()?;
        let state = self.state(current.as_ref(), receipt.as_ref());
        if state == InstallationState::Unmanaged && current.is_some() {
            return Err(InstallError::Unmanaged);
        }
        if state == InstallationState::ModifiedExternally && !overwrite {
            return Err(InstallError::Conflict);
        }
        if current.as_ref() == Some(desired) {
            if receipt
                .as_ref()
                .is_none_or(|r| r.generation.as_ref() != Some(generation) || r.pending.is_some())
            {
                self.save_receipt(&Receipt {
                    generation: Some(generation.clone()),
                    installed: desired.clone(),
                    pending: None,
                    target: self.layout.shell_config.clone(),
                })?;
            }
            return Ok(ApplyOutcome::Unchanged);
        }
        let mut receipt = Receipt {
            generation: Some(generation.clone()),
            installed: current.clone().unwrap_or(Value::Null),
            pending: Some(desired.clone()),
            target: self.layout.shell_config.clone(),
        };
        self.save_receipt(&receipt)?;
        let latest = std::fs::read(&self.layout.shell_config).or_else(|error| {
            if error.kind() == io::ErrorKind::NotFound {
                Ok(Vec::new())
            } else {
                Err(error)
            }
        })?;
        if before != latest {
            return Err(InstallError::Changed);
        }
        AtomicFile::at(&self.layout.shell_config)
            .write(format!("{}\n", serde_json::to_string_pretty(desired)?).as_bytes())?;
        receipt.installed = desired.clone();
        receipt.pending = None;
        self.save_receipt(&receipt)?;
        Ok(ApplyOutcome::Installed)
    }
    fn validate(config: &Value) -> Result<(), InstallError> {
        if !config.is_object() || config.get("version") != Some(&Value::from(1)) {
            return Err(InstallError::Invalid(
                "expected an object with version 1".into(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Fixture {
        root: tempfile::TempDir,
        layout: Layout,
    }
    impl Fixture {
        fn new() -> Self {
            let root = tempfile::tempdir().unwrap();
            let layout = Layout::at(
                root.path().join("config"),
                root.path().join("state"),
                root.path().join("cache"),
            );
            Self { root, layout }
        }
        fn write(&self, value: &Value) {
            AtomicFile::at(&self.layout.shell_config)
                .write(&serde_json::to_vec(value).unwrap())
                .unwrap();
        }
    }
    #[test]
    fn adoption_drift_overwrite_and_rollback() {
        let fixture = Fixture::new();
        let _keep = &fixture.root;
        let old = serde_json::json!({"version":1, "bar":{}});
        let new = serde_json::json!({"version":1, "bar":{"transparent":true}});
        fixture.write(&old);
        let install = ShellInstallation::new(&fixture.layout);
        let a = GenerationId::parse("a").unwrap();
        let b = GenerationId::parse("b").unwrap();
        assert!(matches!(
            install.apply(&new, &a, false),
            Err(InstallError::Unmanaged)
        ));
        install.adopt(&old).unwrap();
        assert_eq!(
            install.apply(&new, &a, false).unwrap(),
            ApplyOutcome::Installed
        );
        assert_eq!(
            install.apply(&new, &a, false).unwrap(),
            ApplyOutcome::Unchanged
        );
        fixture.write(&old);
        assert!(matches!(
            install.apply(&new, &b, false),
            Err(InstallError::Conflict)
        ));
        assert_eq!(install.read().unwrap(), Some(old.clone()));
        install.apply(&new, &b, true).unwrap();
        install.apply(&old, &a, false).unwrap();
        assert_eq!(install.read().unwrap(), Some(old));
    }
}

#[cfg(test)]
mod recovery_tests {
    use super::*;
    #[test]
    fn interrupted_write_is_recoverable_and_backup_is_preserved() {
        let root = tempfile::tempdir().unwrap();
        let layout = Layout::at(
            root.path().join("config"),
            root.path().join("state"),
            root.path().join("cache"),
        );
        let install = ShellInstallation::new(&layout);
        let old = serde_json::json!({"version":1});
        let new = serde_json::json!({"version":1,"plugins":[]});
        AtomicFile::at(&layout.shell_config)
            .write(&serde_json::to_vec(&old).unwrap())
            .unwrap();
        install.adopt(&old).unwrap();
        install
            .save_receipt(&Receipt {
                generation: Some(GenerationId::parse("new").unwrap()),
                installed: old.clone(),
                pending: Some(new.clone()),
                target: layout.shell_config.clone(),
            })
            .unwrap();
        AtomicFile::at(&layout.shell_config)
            .write(&serde_json::to_vec(&new).unwrap())
            .unwrap();
        assert_eq!(install.inspect().unwrap(), InstallationState::Current);
        install
            .apply(&new, &GenerationId::parse("new").unwrap(), false)
            .unwrap();
        assert!(install.receipt().unwrap().unwrap().pending.is_none());
        assert_eq!(
            ShellInstallation::read_json(&layout.shell_backup()).unwrap(),
            Some(old)
        );
    }
    #[test]
    fn invalid_file_and_failed_adoption_are_not_overwritten() {
        let root = tempfile::tempdir().unwrap();
        let layout = Layout::at(
            root.path().join("config"),
            root.path().join("state"),
            root.path().join("cache"),
        );
        let install = ShellInstallation::new(&layout);
        AtomicFile::at(&layout.shell_config)
            .write(b"broken")
            .unwrap();
        assert!(
            install
                .apply(
                    &serde_json::json!({"version":1}),
                    &GenerationId::parse("new").unwrap(),
                    true
                )
                .is_err()
        );
        assert_eq!(std::fs::read(&layout.shell_config).unwrap(), b"broken");
        assert!(!layout.shell_receipt().exists());
    }
}
