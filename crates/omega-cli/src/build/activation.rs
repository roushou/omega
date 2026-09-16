//! Observe acceptance of one published generation under a fixed deadline.
use anyhow::{Context, bail};
use omega_host::{Generations, Layout};
use omega_proto::omega::{DeploymentStatus, ReconciliationState, ShellApplicationState};
use std::time::Duration;

pub(crate) struct Activation {
    pub(crate) timeout: Duration,
}
impl Activation {
    pub(crate) async fn wait_for(
        &self,
        layout: &Layout,
        generation: &omega_host::GenerationId,
        operator: &crate::operator::Operator,
    ) -> anyhow::Result<()> {
        let mut pending = "waiting for the daemon to accept the build".to_string();
        let timeout = self.timeout;
        tokio::time::timeout(timeout, async {
            loop {
                let published = Generations::new(layout).pin_current()?;
                if published.as_ref().map(|build| build.id()) != Some(generation) {
                    bail!("this build was superseded; use omega status to inspect the current build");
                }
                let status = operator.deployment().await
                    .context("cannot inspect build activation; ensure omega daemon is running")?;
                match Self::activation_pending(&status, generation.as_str())? {
                    None => return Ok(()),
                    Some(reason) => pending = reason,
                }
                tokio::time::sleep(Duration::from_millis(250)).await;
            }
        }).await.map_err(|_| anyhow::anyhow!(
            "build activation timed out after {}s: {pending}; use omega status. The build remains published and may activate later",
            timeout.as_secs()
        ))?
    }

    fn activation_pending(
        status: &DeploymentStatus,
        generation: &str,
    ) -> anyhow::Result<Option<String>> {
        if status.candidate_generation == generation && !status.activation_error.is_empty() {
            bail!("build activation failed: {}", status.activation_error);
        }
        if status.shell_generation == generation
            && status.shell == ShellApplicationState::Failed as i32
        {
            bail!(
                "shell application failed: {}; use omega shell diff",
                status.shell_error
            );
        }
        if status.accepted_generation != generation {
            return Ok(Some("waiting for the daemon to accept this build".into()));
        }
        match ReconciliationState::try_from(status.reconciliation) {
            Ok(ReconciliationState::Settled) => {}
            Ok(ReconciliationState::Pending | ReconciliationState::Unspecified) => {
                return Ok(Some(if status.reconciliation_error.is_empty() {
                    "waiting for configuration to be applied".into()
                } else {
                    status.reconciliation_error.clone()
                }));
            }
            Err(_) => bail!("unknown reconciliation state {}", status.reconciliation),
        }
        if status.shell_generation != generation {
            return Ok(Some("waiting for this build's shell result".into()));
        }
        match ShellApplicationState::try_from(status.shell) {
            Ok(ShellApplicationState::Applied | ShellApplicationState::NotDeclared) => Ok(None),
            Ok(ShellApplicationState::Applying | ShellApplicationState::Unspecified) => {
                Ok(Some("waiting for shell application".into()))
            }
            Ok(ShellApplicationState::Failed) => unreachable!("failure handled above"),
            Err(_) => bail!("unknown shell application state {}", status.shell),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn waiting_requires_this_build_and_its_shell_result() {
        let mut status = DeploymentStatus {
            accepted_generation: "old".into(),
            reconciliation: ReconciliationState::Settled as i32,
            shell_generation: "old".into(),
            shell: ShellApplicationState::Applied as i32,
            ..Default::default()
        };
        assert!(
            Activation::activation_pending(&status, "new")
                .unwrap()
                .is_some()
        );
        status.accepted_generation = "new".into();
        assert!(
            Activation::activation_pending(&status, "new")
                .unwrap()
                .is_some()
        );
        status.shell_generation = "new".into();
        assert!(
            Activation::activation_pending(&status, "new")
                .unwrap()
                .is_none()
        );
        status.reconciliation = ReconciliationState::Pending as i32;
        status.reconciliation_error = "plugin not connected".into();
        assert_eq!(
            Activation::activation_pending(&status, "new")
                .unwrap()
                .as_deref(),
            Some("plugin not connected")
        );
    }

    #[test]
    fn activation_and_shell_errors_only_fail_the_matching_build() {
        let mut status = DeploymentStatus {
            candidate_generation: "old".into(),
            activation_error: "invalid document".into(),
            shell_generation: "old".into(),
            shell: ShellApplicationState::Failed as i32,
            shell_error: "external edit".into(),
            ..Default::default()
        };
        assert!(
            Activation::activation_pending(&status, "new")
                .unwrap()
                .is_some()
        );
        status.candidate_generation = "new".into();
        assert!(
            Activation::activation_pending(&status, "new")
                .unwrap_err()
                .to_string()
                .contains("invalid document")
        );
        status.activation_error.clear();
        status.shell_generation = "new".into();
        assert!(
            Activation::activation_pending(&status, "new")
                .unwrap_err()
                .to_string()
                .contains("omega shell diff")
        );
    }

    #[test]
    fn waiting_does_not_invent_a_process_health_gate() {
        let status = DeploymentStatus {
            accepted_generation: "build".into(),
            reconciliation: ReconciliationState::Settled as i32,
            shell_generation: "build".into(),
            shell: ShellApplicationState::NotDeclared as i32,
            units: vec![omega_proto::omega::UnitStatus {
                phase: omega_proto::omega::UnitPhase::Starting as i32,
                ..Default::default()
            }],
            ..Default::default()
        };
        assert!(
            Activation::activation_pending(&status, "build")
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test(start_paused = true)]
    async fn waiting_has_a_deadline_and_never_unpublishes_or_follows_another_build() {
        let root = omega_host::TempPath::sibling(&std::env::temp_dir().join("omega-wait"), "test");
        let layout = Layout::at(root.join("config"), root.join("state"), root.join("cache"));
        let store = Generations::new(&layout);
        let stage = store.stage().unwrap();
        let id = stage.id();
        stage.commit().unwrap();
        let socket = omega_proto::Socket::at(root.join("control.sock"));
        let listener = socket.bind().unwrap();
        let operator = crate::operator::Operator::at(socket);
        let cmd = Activation {
            timeout: Duration::from_secs(1),
        };
        let started = tokio::time::Instant::now();
        let error = cmd.wait_for(&layout, &id, &operator).await.unwrap_err();
        assert!(error.to_string().contains("timed out"), "{error}");
        assert_eq!(started.elapsed(), Duration::from_secs(1));
        assert_eq!(store.pin_current().unwrap().unwrap().id(), &id);

        let other = store.stage().unwrap();
        let other_id = other.id();
        other.commit().unwrap();
        let error = cmd.wait_for(&layout, &id, &operator).await.unwrap_err();
        assert!(error.to_string().contains("superseded"), "{error}");
        assert_eq!(store.pin_current().unwrap().unwrap().id(), &other_id);
        drop(listener);
        std::fs::remove_dir_all(root).unwrap();
    }
}
