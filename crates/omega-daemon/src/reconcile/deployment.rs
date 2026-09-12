//! Live progress projected by activation and shell application, never inferred from files.
use omega_host::GenerationId;
use omega_proto::omega::{DeploymentStatus, ReconciliationState, ShellApplicationState};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Default)]
pub struct Deployment(Arc<Mutex<DeploymentStatus>>);

impl Deployment {
    pub fn snapshot(&self) -> DeploymentStatus {
        self.lock().clone()
    }

    pub(super) fn candidate(&self, generation: &GenerationId) {
        let mut state = self.lock();
        state.candidate_generation = generation.to_string();
        state.activation_error.clear();
    }

    pub(super) fn activation_failed(&self, error: impl std::fmt::Display) {
        self.lock().activation_error = error.to_string();
    }

    pub(super) fn accepted(&self, generation: &GenerationId) {
        let mut state = self.lock();
        state.accepted_generation = generation.to_string();
        if state.candidate_generation == state.accepted_generation {
            state.activation_error.clear();
        }
        state.reconciliation = ReconciliationState::Pending as i32;
        state.reconciliation_error.clear();
    }

    pub(super) fn pending(&self) {
        let mut state = self.lock();
        if !state.accepted_generation.is_empty() {
            state.reconciliation = ReconciliationState::Pending as i32;
        }
    }

    pub(super) fn reconciled(&self, error: Option<&crate::DaemonError>) {
        let mut state = self.lock();
        if state.accepted_generation.is_empty() {
            return;
        }
        state.reconciliation = if error.is_some() {
            ReconciliationState::Pending
        } else {
            ReconciliationState::Settled
        } as i32;
        state.reconciliation_error = error.map(ToString::to_string).unwrap_or_default();
    }

    pub(super) fn applying_shell(&self, generation: &GenerationId, declared: bool) {
        let mut state = self.lock();
        state.shell_generation = generation.to_string();
        state.shell = if declared {
            ShellApplicationState::Applying
        } else {
            ShellApplicationState::NotDeclared
        } as i32;
        state.shell_error.clear();
    }

    pub(super) fn shell_applied(&self, error: Option<&super::shell::ShellApplyError>) {
        let mut state = self.lock();
        state.shell = if error.is_some() {
            ShellApplicationState::Failed
        } else {
            ShellApplicationState::Applied
        } as i32;
        state.shell_error = error.map(ToString::to_string).unwrap_or_default();
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, DeploymentStatus> {
        self.0.lock().unwrap_or_else(|error| error.into_inner())
    }
}
