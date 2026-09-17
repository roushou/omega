use super::{Change, Observation, Receipt, RecoveryError, RecoveryStore, Replacement};
use std::{io, path::PathBuf};

/// Result of a verified replacement. Unchanged targets are checked without
/// writing a record or allocating a backup. Changed targets retain a receipt.
#[derive(Debug)]
pub struct InstalledReplacement {
    pub target: PathBuf,
    pub outcome: ReplacementOutcome,
    pub recovery: Option<RetainedChange>,
}

/// The verified effect of installing a replacement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplacementOutcome {
    Created,
    Updated,
    Unchanged,
}

/// A durable replacement's recovery information.
#[derive(Debug)]
pub struct RetainedChange {
    pub record: PathBuf,
    pub receipt: Receipt,
}

impl Replacement {
    /// Durably install a replacement and retain its recovery record. Unchanged
    /// targets are checked without writing a record. Failures never trigger
    /// automatic rollback; the saved change owns intent and postcondition checks.
    pub fn install(
        self,
        store: &RecoveryStore,
    ) -> Result<InstalledReplacement, RecoveryError<io::Error>> {
        let target = self.target().to_path_buf();
        if !self.changed() {
            if self.inspect().map_err(RecoveryError::Change)? != Observation::Unchanged {
                return Err(RecoveryError::Conflict);
            }
            return Ok(InstalledReplacement {
                target,
                outcome: ReplacementOutcome::Unchanged,
                recovery: None,
            });
        }
        let outcome = if self.creates_target() {
            ReplacementOutcome::Created
        } else {
            ReplacementOutcome::Updated
        };
        let mut saved = store.prepare(self)?;
        saved.apply()?;
        Ok(InstalledReplacement {
            target,
            outcome,
            recovery: Some(RetainedChange {
                record: saved.path().into(),
                receipt: saved.receipt().clone(),
            }),
        })
    }
}
