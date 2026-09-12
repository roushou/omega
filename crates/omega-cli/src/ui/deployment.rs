use super::{Step, Ui};
use omega_proto::omega::{DeploymentStatus, ReconciliationState, ShellApplicationState};

impl Ui {
    /// Report a live daemon snapshot separately from the locally published selection.
    pub fn deployment(&mut self, status: &DeploymentStatus, published: Option<&str>) {
        match published {
            Some(_) => self.step(Step::Built, "configuration published"),
            None => self.detail("No published build available."),
        }
        if status.accepted_generation.is_empty() {
            self.detail("The daemon has not accepted a build yet.");
        } else {
            self.step(Step::Checked, "the daemon has accepted a build");
        }
        if published.is_some_and(|id| id != status.accepted_generation) {
            self.detail("The latest build is not active yet.");
        }
        if !status.activation_error.is_empty() {
            self.warn(format!("build activation: {}", status.activation_error));
        }
        match ReconciliationState::try_from(status.reconciliation) {
            Ok(ReconciliationState::Settled) => self.step(
                Step::Checked,
                "last reconciliation pass completed; plugin phases are below",
            ),
            Ok(ReconciliationState::Pending) => {
                self.step(Step::Checking, "reconciliation pending");
                if !status.reconciliation_error.is_empty() {
                    self.detail(&status.reconciliation_error);
                }
            }
            Ok(ReconciliationState::Unspecified) => self.detail("No reconciliation result yet."),
            Err(_) => self.warn(format!(
                "unknown reconciliation state {}",
                status.reconciliation
            )),
        }
        match ShellApplicationState::try_from(status.shell) {
            Ok(ShellApplicationState::Applied) => {
                self.step(Step::Installed, "last shell application succeeded")
            }
            Ok(ShellApplicationState::NotDeclared) => {
                self.detail("Accepted document declares no shell.")
            }
            Ok(ShellApplicationState::Applying) => {
                self.step(Step::Checking, "applying shell configuration")
            }
            Ok(ShellApplicationState::Failed) => {
                self.warn(format!("shell application failed: {}", status.shell_error));
                self.next("omega shell diff");
            }
            Ok(ShellApplicationState::Unspecified) => {
                self.detail("No shell application result yet.")
            }
            Err(_) => self.warn(format!("unknown shell application state {}", status.shell)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejection_and_shell_failure_do_not_hide_the_accepted_build() {
        let (mut ui, transcript) = Ui::recording();
        ui.deployment(
            &DeploymentStatus {
                candidate_generation: "new".into(),
                accepted_generation: "old".into(),
                activation_error: "invalid document".into(),
                reconciliation: ReconciliationState::Settled as i32,
                shell_generation: "old".into(),
                shell: ShellApplicationState::Failed as i32,
                shell_error: "external edit".into(),
                ..Default::default()
            },
            Some("new"),
        );
        let text = transcript.err();
        for expected in [
            "configuration published",
            "the daemon has accepted a build",
            "invalid document",
            "last reconciliation pass completed",
            "shell application failed: external edit",
            "omega shell diff",
        ] {
            assert!(text.contains(expected), "{text}");
        }
        assert!(transcript.out().is_empty());
    }
}
