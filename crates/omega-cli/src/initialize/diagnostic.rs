use super::Initialize;
use crate::ui::Paint;
use omega_base::execution::{Failure, FailureCause, Outcome, Report, StepId};
use omega_host::recovery::{ChangeId, RecoveryError};

#[derive(Debug, thiserror::Error, miette::Diagnostic)]
#[error("initialization stopped at: {step}")]
#[diagnostic(code(omega::init::failed), help("{help}"))]
pub(crate) struct InitFailure {
    step: String,
    #[source]
    pub(crate) source: anyhow::Error,
    help: String,
}

impl InitFailure {
    pub(super) fn new(
        failure: Failure<anyhow::Error>,
        reports: &[Report],
        request: &Initialize,
    ) -> Self {
        let (source, mut help) = match failure.cause {
            FailureCause::Operation(source) => {
                let help = Self::recovery(&source)
                    .unwrap_or_else(|| Self::guidance(failure.step.id, request));
                (source, help)
            }
            FailureCause::Unconfigured => (
                anyhow::anyhow!("no implementation configured for {}", failure.step.id.0),
                "This is an internal initialization error. Report the failed step and your Omega version.".into(),
            ),
        };

        if reports.iter().any(|report| {
            report.description.id.0 == "build.publish" && report.outcome == Outcome::Completed
        }) {
            help.push_str("\nA new generation was published and remains selected. Inspect it with omega status; omega rollback can select a previous accepted generation if one exists.");
        }

        Self {
            step: failure.step.title,
            source,
            help,
        }
    }

    fn recovery(source: &anyhow::Error) -> Option<String> {
        for cause in source.chain() {
            let Some(error) = cause.downcast_ref::<RecoveryError<std::io::Error>>() else {
                continue;
            };
            let Some(path) = error.record_path() else {
                continue;
            };
            let Some(id) = path.file_stem().and_then(|id| id.to_str()) else {
                return Some(format!(
                    "Inspect the retained recovery record at {} before retrying.",
                    Paint::abbreviate(path).display()
                ));
            };
            let Ok(id) = ChangeId::parse(id) else {
                return Some(format!(
                    "Inspect the retained recovery record at {} before retrying.",
                    Paint::abbreviate(path).display()
                ));
            };
            return Some(format!(
                "Inspect the retained change first: omega recovery inspect {id}\nIf the intended write completed, confirm it with omega recovery accept {id}. To undo it, stop the daemon and run omega recovery restore {id}. Recovery refuses conflicting external edits."
            ));
        }
        None
    }

    fn guidance(step: StepId, request: &Initialize) -> String {
        let retry = if request.bare {
            "omega init --bare"
        } else if request.profile == omega_host::Profile::Debug {
            "omega init --debug"
        } else {
            "omega init"
        };

        let advice = match step.0 {
            "init.preflight" => {
                "Resolve the requirement reported above. Use omega init --bare if you only want to create the Rust workspace."
            }
            "init.workspace.prepare" => {
                "Correct the workspace or shell configuration reported above. For shell ownership conflicts, run omega shell diff before adopting or applying changes."
            }
            "init.adopt" => {
                "Inspect omega shell diff and the shell backup shown in the setup summary before retrying adoption."
            }
            "init.workspace.write"
            | "init.dependencies"
            | "init.renderer.install"
            | "init.service.install" => {
                "Check permissions and available space at the reported target. Preserve any external edits before retrying. Confirmed file changes and their recovery commands are listed in the setup summary."
            }
            "build.compile" => {
                "Fix the Cargo error above in your configuration workspace. The generated sources have been retained; rerunning initialization will keep existing files."
            }
            "build.describe" => {
                "Fix the plugin manifest error above, then rebuild. Renderer and service installation have not started."
            }
            "build.evaluate" => {
                "Correct the system document or shell configuration reported above. Renderer and service installation have not started."
            }
            "init.daemon.activate" | "init.daemon.verify" => {
                "Inspect systemctl --user status omega.service and journalctl --user -u omega.service -n 50. Installed files are retained; the service may have started even if verification failed."
            }
            "build.publish" => {
                "Publication may have completed. Run omega status before retrying and check permissions and available space in the state directory."
            }
            "init.generation.verify" => {
                "Run omega status to inspect daemon convergence and omega shell diff to inspect shell conflicts."
            }
            "init.shell.restart" => {
                "Inspect the shell error above, then run omarchy restart shell and omega shell status."
            }
            "init.renderer.verify" => {
                "Run omega shell status. If the running renderer is stale, use omega shell install to reinstall it and restart the shell."
            }
            "init.build.prepare" | "init.bare" => {
                "Inspect the initialization error above before retrying."
            }
            _ => "Inspect the failed operation above before retrying.",
        };

        format!("{advice}\nAfter resolving the problem, rerun {retry}.")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::Ui;
    use omega_base::execution::Description;
    use omega_host::{Layout, Profile};
    use omega_proto::Socket;

    struct Fixture;

    impl Fixture {
        fn request() -> Initialize {
            Initialize {
                layout: Layout::at("/config", "/state", "/cache"),
                bare: false,
                profile: Profile::Debug,
                socket: Socket::at("/socket"),
            }
        }

        fn failure(id: &'static str, source: anyhow::Error, reports: &[Report]) -> InitFailure {
            InitFailure::new(
                Failure {
                    step: Description::new(id, "test operation"),
                    cause: FailureCause::Operation(source),
                },
                reports,
                &Self::request(),
            )
        }

        fn render(failure: InitFailure) -> String {
            let (mut ui, transcript) = Ui::recording();
            ui.error(&failure.into());
            assert!(transcript.out().is_empty());
            assert!(!transcript.err().contains('\u{1b}'));
            transcript.err()
        }
    }

    #[test]
    fn preflight_failure_has_actionable_help_without_claiming_changes_or_backups() {
        let failure = Fixture::failure(
            "init.preflight",
            anyhow::anyhow!("cargo is unavailable"),
            &[],
        );
        assert!(failure.help.contains("omega init --debug"));
        assert!(!failure.help.contains("recovery"));
        assert!(!failure.help.contains("retained"));
        let text = Fixture::render(failure);
        assert!(text.contains("omega::init::failed"), "{text}");
        assert!(
            text.contains("initialization stopped at: test operation"),
            "{text}"
        );
        assert!(text.contains("cargo is unavailable"), "{text}");
    }

    #[test]
    fn nested_recovery_failure_selects_the_exact_record_before_retry_advice() {
        let source = anyhow::Error::new(RecoveryError::<std::io::Error>::Recorded {
            path: "/state/recovery/change-123.json".into(),
            source: Box::new(RecoveryError::Change(std::io::Error::other("disk full"))),
        })
        .context("installing configuration");
        let failure = Fixture::failure("init.workspace.write", source, &[]);
        assert!(failure.help.contains("omega recovery inspect change-123"));
        assert!(failure.help.contains("omega recovery accept change-123"));
        assert!(failure.help.contains("omega recovery restore change-123"));
        assert!(!failure.help.contains("rerun omega init"));
        let text = Fixture::render(failure);
        assert!(text.contains("disk full"), "{text}");
        assert!(text.contains("installing configuration"), "{text}");
    }

    #[test]
    fn publication_completion_changes_failure_guidance() {
        let reports = [Report {
            description: Description::new("build.publish", "publish"),
            outcome: Outcome::Completed,
            details: vec![],
        }];
        let failure = Fixture::failure(
            "init.shell.restart",
            anyhow::anyhow!("restart refused"),
            &reports,
        );
        assert!(failure.help.contains("omarchy restart shell"));
        assert!(failure.help.contains("remains selected"));
        assert!(failure.help.contains("if one exists"));

        let failure = Fixture::failure("build.publish", anyhow::anyhow!("sync failed"), &[]);
        assert!(failure.help.contains("may have completed"));
        assert!(!failure.help.contains("remains selected"));
    }

    #[test]
    fn initialization_keeps_nested_source_labels_and_context() {
        let source = omega_omarchy::shell::Shell::from_omarchy("{broken").unwrap_err();
        let failure = Fixture::failure(
            "init.workspace.prepare",
            anyhow::Error::new(source).context("reading the existing shell configuration"),
            &[],
        );
        let text = Fixture::render(failure);
        assert!(text.contains("omega::init::failed"), "{text}");
        assert!(text.contains("shell.json"), "{text}");
        assert!(text.contains("{broken"), "{text}");
        assert!(text.contains("invalid JSON"), "{text}");
        assert!(
            text.contains("reading the existing shell configuration"),
            "{text}"
        );
        assert!(text.contains("omega init --debug"), "{text}");
    }
}
