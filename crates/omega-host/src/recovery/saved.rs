use super::record::Record;
use super::{Change, Observation, Receipt, State, policy};
use crate::{AtomicFile, fs::FileLock};
use std::{
    io,
    path::{Path, PathBuf},
};

/// Failures retain the on-disk record. An I/O error can occur after an effect;
/// inspect the record and target before deciding to retry or restore.
#[derive(Debug, thiserror::Error)]
pub enum RecoveryError<E: std::error::Error + 'static> {
    #[error("recovery storage failed: {0}")]
    Io(#[from] io::Error),
    #[error("invalid recovery record: {0}")]
    Decode(#[from] serde_json::Error),
    #[error("unsupported or mismatched recovery record")]
    InvalidRecord,
    #[error("unfinished change at {}; inspect or restore it before starting another change", .0.display())]
    Pending(PathBuf),
    #[error("change conflicts with current state; preserve external edits")]
    Conflict,
    #[error("journal durability is uncertain; drop this handle and reopen {}", .0.display())]
    ReopenRequired(PathBuf),
    #[error("the operation completed without reaching its expected state")]
    Postcondition,
    #[error("this change cannot be applied in its current state")]
    InvalidState,
    #[error(transparent)]
    Change(E),
    #[error("change outcome requires inspection; recovery record: {}", path.display())]
    Recorded {
        path: PathBuf,
        #[source]
        source: Box<Self>,
    },
}

impl<E: std::error::Error + 'static> RecoveryError<E> {
    /// The retained record requiring inspection, when this failure identifies one.
    pub fn record_path(&self) -> Option<&Path> {
        match self {
            Self::Pending(path) | Self::ReopenRequired(path) | Self::Recorded { path, .. } => {
                Some(path)
            }
            Self::Io(_)
            | Self::Decode(_)
            | Self::InvalidRecord
            | Self::Conflict
            | Self::Postcondition
            | Self::InvalidState
            | Self::Change(_) => None,
        }
    }
}

/// Owns a durable record and its exclusive lease. Restore is idempotent when
/// the before-state is already present. External changes are never overwritten.
#[derive(Debug)]
pub struct SavedChange<C: Change> {
    pub(super) path: PathBuf,
    pub(super) record: Record<C>,
    pub(super) _lock: FileLock,
    pub(super) uncertain: bool,
    #[cfg(test)]
    pub(super) fail_after_save: Option<State>,
}

impl<C: Change> SavedChange<C> {
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn change(&self) -> &C {
        &self.record.change
    }

    /// Last acknowledged journal state. After a journal-write failure, drop and
    /// reopen the handle before treating this state as authoritative.
    pub fn receipt(&self) -> &Receipt {
        &self.record.receipt
    }

    pub fn inspect(&self) -> Result<Observation, RecoveryError<C::Error>> {
        self.record.change.inspect().map_err(RecoveryError::Change)
    }

    pub(super) fn save(&self) -> Result<(), RecoveryError<C::Error>> {
        use std::os::unix::fs::PermissionsExt;
        AtomicFile::at(&self.path).write_with_permissions(
            &serde_json::to_vec(&self.record)?,
            std::fs::Permissions::from_mode(0o600),
        )?;
        #[cfg(test)]
        if self.fail_after_save == Some(self.record.receipt.state) {
            return Err(io::Error::other("journal acknowledgment failed").into());
        }
        Ok(())
    }

    pub(super) fn recorded(&self, source: RecoveryError<C::Error>) -> RecoveryError<C::Error> {
        RecoveryError::Recorded {
            path: self.path.clone(),
            source: Box::new(source),
        }
    }

    fn ensure_certain(&self) -> Result<(), RecoveryError<C::Error>> {
        if self.uncertain {
            return Err(RecoveryError::ReopenRequired(self.path.clone()));
        }
        Ok(())
    }

    pub(super) fn transition(&mut self, state: State) -> Result<(), RecoveryError<C::Error>> {
        self.ensure_certain()?;

        let previous = self.record.receipt.state;
        self.record.receipt.state = state;

        if let Err(error) = self.save() {
            self.record.receipt.state = previous;
            self.uncertain = true;
            return Err(error);
        }
        Ok(())
    }

    fn decide(
        &self,
        operation: policy::Operation,
    ) -> Result<policy::Decision, RecoveryError<C::Error>> {
        self.ensure_certain()?;
        policy::Policy::decide(self.record.receipt.state, self.inspect()?, operation).map_err(
            |error| match error {
                policy::DecisionError::State => RecoveryError::InvalidState,
                policy::DecisionError::Conflict => RecoveryError::Conflict,
            },
        )
    }

    fn confirm(&self, after: bool) -> Result<(), RecoveryError<C::Error>> {
        let observed = self.inspect()?;
        let matches = if after {
            matches!(observed, Observation::After | Observation::Unchanged)
        } else {
            matches!(observed, Observation::Before | Observation::Unchanged)
        };
        if !matches {
            return Err(RecoveryError::Postcondition);
        }

        self.record
            .change
            .confirm(observed)
            .map_err(RecoveryError::Change)
    }

    pub fn apply(&mut self) -> Result<(), RecoveryError<C::Error>> {
        self.decide(policy::Operation::Apply)?;

        let result = (|| {
            self.transition(State::Applying)?;
            self.record.change.apply().map_err(RecoveryError::Change)?;
            if !matches!(self.inspect()?, Observation::After | Observation::Unchanged) {
                return Err(RecoveryError::Postcondition);
            }
            self.transition(State::Applied)
        })();

        result.map_err(|error| self.recorded(error))
    }

    /// Acknowledge an interrupted apply only after confirming its after-state
    /// and durability. This never executes the original operation again.
    pub fn accept(&mut self) -> Result<(), RecoveryError<C::Error>> {
        self.decide(policy::Operation::Accept)?;

        let result = (|| {
            self.confirm(true)?;
            self.transition(State::Applied)
        })();

        result.map_err(|error| self.recorded(error))
    }

    pub fn restore(&mut self) -> Result<(), RecoveryError<C::Error>> {
        let decision = self.decide(policy::Operation::Restore)?;

        let result = (|| {
            match decision {
                policy::Decision::Restore => {
                    self.transition(State::Restoring)?;
                    self.record
                        .change
                        .restore()
                        .map_err(RecoveryError::Change)?;
                    if !matches!(
                        self.inspect()?,
                        Observation::Before | Observation::Unchanged
                    ) {
                        return Err(RecoveryError::Postcondition);
                    }
                }
                policy::Decision::FinishRestoring => {
                    self.confirm(false)?;
                }
                policy::Decision::AlreadyRestored => return Ok(()),
                policy::Decision::Apply | policy::Decision::Accept => {
                    unreachable!("restore policy returned a different operation")
                }
            }
            self.transition(State::Restored)
        })();

        result.map_err(|error| self.recorded(error))
    }
}
