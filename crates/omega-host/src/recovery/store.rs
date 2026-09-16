use super::record::{Record, RecordHeader, VERSION};
use super::{Change, ChangeId, Observation, Receipt, RecoveryError, SavedChange, State};
use crate::{Directory, Layout, fs::FileLock};
use std::{io, path::Path};

/// A private store selected through Layout. Handles hold an exclusive store lock
/// across inspection and effects. Completed records remain available for recovery.
#[derive(Debug, Clone)]
pub struct RecoveryStore {
    layout: Layout,
}

impl RecoveryStore {
    pub fn new(layout: &Layout) -> Self {
        Self {
            layout: layout.clone(),
        }
    }

    fn lock(&self) -> io::Result<FileLock> {
        use std::os::unix::fs::PermissionsExt;
        let directory = self.layout.recovery_dir();
        match std::fs::symlink_metadata(&directory) {
            Ok(metadata) if !metadata.is_dir() => {
                return Err(io::Error::other("recovery store must be a real directory"));
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                Directory::create_all(&directory)?
            }
            Err(error) => return Err(error),
        }

        std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700))?;
        Directory::sync(&directory)?;
        FileLock::exclusive(&self.layout.recovery_lock())
    }

    fn reader(path: &Path) -> io::Result<io::BufReader<std::fs::File>> {
        use std::os::unix::fs::OpenOptionsExt;
        let file = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(path)?;
        if !file.metadata()?.is_file() {
            return Err(io::Error::other("recovery record must be a regular file"));
        }
        Ok(io::BufReader::new(file))
    }

    pub fn receipts(&self) -> Result<Vec<Receipt>, RecoveryError<io::Error>> {
        let _lock = self.lock()?;
        self.read_receipts().map_err(RecoveryError::Io)
    }

    /// Refuse unfinished changes before a workflow starts making new changes.
    /// A missing store is empty; this check does not create it. Preparation
    /// repeats the check under the store lock before allowing an effect.
    pub fn check_pending(&self) -> Result<(), RecoveryError<io::Error>> {
        if !self.layout.recovery_dir().try_exists()? {
            return Ok(());
        }

        for receipt in self.receipts()? {
            if !matches!(receipt.state, State::Applied | State::Restored) {
                return Err(RecoveryError::Pending(
                    self.layout.recovery_record(&receipt.id),
                ));
            }
        }
        Ok(())
    }

    fn read_receipts(&self) -> io::Result<Vec<Receipt>> {
        let mut receipts = Vec::new();
        for entry in std::fs::read_dir(self.layout.recovery_dir())? {
            let entry = entry?;
            if entry.path().extension().is_some_and(|ext| ext == "json") {
                let record: RecordHeader = serde_json::from_reader(Self::reader(&entry.path())?)?;
                if record.version != VERSION
                    || entry.path() != self.layout.recovery_record(&record.receipt.id)
                {
                    return Err(io::Error::other("invalid recovery record identity"));
                }
                receipts.push(record.receipt);
            }
        }

        receipts.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(receipts)
    }

    /// Persist the recovery payload before allowing effects. An unfinished record
    /// blocks new changes; opening it for inspection or recovery remains allowed.
    pub fn prepare<C: Change>(&self, change: C) -> Result<SavedChange<C>, RecoveryError<C::Error>> {
        let lock = self.lock()?;

        for receipt in self.read_receipts()? {
            if !matches!(receipt.state, State::Applied | State::Restored) {
                return Err(RecoveryError::Pending(
                    self.layout.recovery_record(&receipt.id),
                ));
            }
        }

        if !matches!(
            change.inspect().map_err(RecoveryError::Change)?,
            Observation::Before | Observation::Unchanged
        ) {
            return Err(RecoveryError::Conflict);
        }

        let id = self.layout.new_change_id();
        let record = Record {
            version: VERSION,
            receipt: Receipt {
                id: id.clone(),
                kind: C::KIND.into(),
                state: State::Prepared,
            },
            change,
        };

        let saved = SavedChange {
            path: self.layout.recovery_record(&id),
            record,
            _lock: lock,
            uncertain: false,
            #[cfg(test)]
            fail_after_save: None,
        };
        saved.save().map_err(|error| saved.recorded(error))?;
        Ok(saved)
    }

    pub fn open<C: Change>(
        &self,
        id: &ChangeId,
    ) -> Result<SavedChange<C>, RecoveryError<C::Error>> {
        let lock = self.lock()?;
        let path = self.layout.recovery_record(id);
        use std::io::Read;
        let mut bytes = Vec::new();
        Self::reader(&path)?.read_to_end(&mut bytes)?;
        let header: RecordHeader = serde_json::from_slice(&bytes)?;
        if header.version != VERSION || header.receipt.kind != C::KIND || &header.receipt.id != id {
            return Err(RecoveryError::InvalidRecord);
        }

        let record: Record<C> = serde_json::from_slice(&bytes)?;
        Ok(SavedChange {
            path,
            record,
            _lock: lock,
            uncertain: false,
            #[cfg(test)]
            fail_after_save: None,
        })
    }
}
