use super::{
    InitReport, Initialize,
    preflight::{Host, HostPlan, Prepared},
    summary::Changes,
};
use crate::{
    build::{
        activation::Activation,
        steps::{Published, Sources, Validated},
    },
    checkout::CheckoutLink,
    operator::Operator,
    renderer::{RendererStatus, Snapshot},
    service::DaemonService,
    workspace::ConfigWorkspace,
};
use anyhow::{Context, ensure};
use omega_base::execution::{Operation, Progress};
use omega_host::recovery::{RecoveryStore, Replacement};
use omega_host::systemd::Installed;
use omega_omarchy::installation::ShellInstallation;
use omega_proto::Socket;
use std::time::Duration;

pub(super) struct WorkspaceReady {
    pub(super) request: Initialize,
    pub(super) workspace: ConfigWorkspace,
    pub(super) host: Option<HostPlan>,
}

pub(super) struct Installation {
    runtime: Runtime,
    service: Replacement,
    renderers: Vec<Replacement>,
}

pub(super) struct ServicePending {
    runtime: Runtime,
    service: Replacement,
}

pub(super) struct Runtime {
    host: Host,
    socket: Socket,
}

pub(super) struct Applied {
    pub(super) runtime: Runtime,
    pub(super) published: Published,
    pub(super) before: Snapshot,
}

pub(super) struct AdoptShell(pub(super) Changes);

impl Operation<Prepared> for AdoptShell {
    type Output = Prepared;
    type Error = anyhow::Error;

    async fn execute(
        self,
        prepared: Prepared,
        progress: &mut Progress<'_>,
    ) -> anyhow::Result<Prepared> {
        if let Some(expected) = &prepared.imported {
            let layout = prepared.workspace.layout();
            let adopted = ShellInstallation::new(layout).adopt(expected);
            let backup = self.record_backup(layout);
            if backup.is_err() {
                self.0.unverified(layout.shell_backup());
            }
            adopted?;
            backup?;
            progress.path("Original shell backup:", layout.shell_backup());
            progress.path("Shell ownership receipt:", layout.shell_receipt());
        } else {
            if !prepared.request.bare {
                self.record_backup(prepared.workspace.layout())?;
            }
            progress.skip("no shell import to adopt");
        }
        Ok(prepared)
    }
}

impl AdoptShell {
    fn record_backup(&self, layout: &omega_host::Layout) -> std::io::Result<()> {
        let backup = layout.shell_backup();
        if backup.try_exists()? {
            self.0.shell_backup(backup, layout.shell_config.clone());
        }
        Ok(())
    }
}

pub(super) struct WriteWorkspace(pub(super) Changes);

impl Operation<Prepared> for WriteWorkspace {
    type Output = Prepared;
    type Error = anyhow::Error;

    async fn execute(
        self,
        mut prepared: Prepared,
        progress: &mut Progress<'_>,
    ) -> anyhow::Result<Prepared> {
        Files::install(
            std::mem::take(&mut prepared.files),
            &RecoveryStore::new(prepared.workspace.layout()),
            progress,
            &self.0,
        )?;
        Ok(prepared)
    }
}

pub(super) struct ConfigureDependencies(pub(super) Changes);

impl Operation<Prepared> for ConfigureDependencies {
    type Output = WorkspaceReady;
    type Error = anyhow::Error;

    async fn execute(
        self,
        prepared: Prepared,
        progress: &mut Progress<'_>,
    ) -> anyhow::Result<WorkspaceReady> {
        if let Some(source) = prepared.source {
            let link = CheckoutLink::new(&prepared.workspace).prepare(Some(&source))?;
            Files::install(
                link.replacements()?,
                &RecoveryStore::new(prepared.workspace.layout()),
                progress,
                &self.0,
            )?;
            progress.path("Linked checkout:", source.root());
        } else {
            progress.skip("keeping existing dependency sources");
        }
        Ok(WorkspaceReady {
            request: prepared.request,
            workspace: prepared.workspace,
            host: prepared.host,
        })
    }
}

pub(super) struct FinishBare;

impl Operation<WorkspaceReady> for FinishBare {
    type Output = InitReport;
    type Error = anyhow::Error;

    async fn execute(
        self,
        ready: WorkspaceReady,
        _: &mut Progress<'_>,
    ) -> anyhow::Result<InitReport> {
        Ok(InitReport {
            layout: ready.request.layout,
            renderer: None,
        })
    }
}

pub(super) struct PrepareBuild;

impl Operation<WorkspaceReady> for PrepareBuild {
    type Output = (Installation, Sources);
    type Error = anyhow::Error;

    async fn execute(
        self,
        ready: WorkspaceReady,
        _: &mut Progress<'_>,
    ) -> anyhow::Result<Self::Output> {
        let plan = ready
            .host
            .context("desktop initialization requires a prepared host installation")?;
        let runtime = Runtime {
            host: plan.host,
            socket: ready.request.socket,
        };
        Ok((
            Installation {
                runtime,
                service: plan.service,
                renderers: plan.renderers,
            },
            Sources {
                workspace: ready.workspace,
                profile: ready.request.profile,
            },
        ))
    }
}

pub(super) struct InstallRenderer(pub(super) Changes);

impl Operation<(Installation, Validated)> for InstallRenderer {
    type Output = (ServicePending, Validated);
    type Error = anyhow::Error;

    async fn execute(
        self,
        (installation, build): (Installation, Validated),
        progress: &mut Progress<'_>,
    ) -> anyhow::Result<Self::Output> {
        Files::install(
            installation.renderers,
            &RecoveryStore::new(build.layout()),
            progress,
            &self.0,
        )?;
        Ok((
            ServicePending {
                runtime: installation.runtime,
                service: installation.service,
            },
            build,
        ))
    }
}

pub(super) struct InstallService(pub(super) Changes);

impl Operation<(ServicePending, Validated)> for InstallService {
    type Output = (Runtime, Validated);
    type Error = anyhow::Error;

    async fn execute(
        self,
        (pending, build): (ServicePending, Validated),
        progress: &mut Progress<'_>,
    ) -> anyhow::Result<Self::Output> {
        Files::install(
            [pending.service],
            &RecoveryStore::new(build.layout()),
            progress,
            &self.0,
        )?;
        Ok((pending.runtime, build))
    }
}

pub(super) struct StartDaemon;

impl Operation<(Runtime, Validated)> for StartDaemon {
    type Output = (Runtime, Validated);
    type Error = anyhow::Error;

    async fn execute(
        self,
        input: Self::Output,
        _: &mut Progress<'_>,
    ) -> anyhow::Result<Self::Output> {
        let runtime = &input.0;
        ensure!(
            !runtime.socket.is_live() || runtime.host.service.status().await?.active.is_active(),
            "a foreground daemon appeared during initialization; stop it before retrying"
        );
        DaemonService::activate(&runtime.host.service).await?;
        Ok(input)
    }
}

pub(super) struct VerifyDaemon;

impl Operation<(Runtime, Validated)> for VerifyDaemon {
    type Output = (Runtime, Validated);
    type Error = anyhow::Error;

    async fn execute(
        self,
        input: Self::Output,
        _: &mut Progress<'_>,
    ) -> anyhow::Result<Self::Output> {
        let runtime = &input.0;
        ensure!(
            runtime.host.service.installed(&runtime.host.definition)? == Installed::Current,
            "the daemon service file changed during initialization"
        );

        let operator = Operator::at(runtime.socket.clone());
        tokio::time::timeout(Duration::from_secs(15), async {
            loop {
                if let Ok(version) = operator.daemon_version().await {
                    ensure!(
                        version == env!("CARGO_PKG_VERSION"),
                        "daemon version {version} differs from this CLI ({}); check omega daemon status",
                        env!("CARGO_PKG_VERSION")
                    );

                    return Ok::<(), anyhow::Error>(());
                }

                tokio::time::sleep(Duration::from_millis(250)).await;
            }
        })
        .await
        .context(
            "daemon did not become ready within 15s; run systemctl --user status omega.service",
        )??;

        Ok(input)
    }
}

pub(super) struct VerifyApplication;

impl Operation<(Runtime, Published)> for VerifyApplication {
    type Output = Applied;
    type Error = anyhow::Error;

    async fn execute(
        self,
        (runtime, published): (Runtime, Published),
        _: &mut Progress<'_>,
    ) -> anyhow::Result<Applied> {
        let operator = Operator::at(runtime.socket.clone());
        Activation {
            timeout: Duration::from_secs(30),
        }
        .wait_for(&published.layout, &published.generation, &operator)
        .await?;
        let before = RendererStatus::read_at(&operator).await?;
        Ok(Applied {
            runtime,
            published,
            before,
        })
    }
}

pub(super) struct RestartShell;

impl Operation<Applied> for RestartShell {
    type Output = Applied;
    type Error = anyhow::Error;

    async fn execute(self, applied: Applied, _: &mut Progress<'_>) -> anyhow::Result<Applied> {
        RendererStatus::restart(applied.runtime.host.shell).await?;
        Ok(applied)
    }
}

pub(super) struct VerifyRenderer;

impl Operation<Applied> for VerifyRenderer {
    type Output = InitReport;
    type Error = anyhow::Error;

    async fn execute(self, applied: Applied, _: &mut Progress<'_>) -> anyhow::Result<InitReport> {
        let renderer =
            RendererStatus::verify(&applied.before, &Operator::at(applied.runtime.socket)).await?;
        Ok(InitReport {
            layout: applied.published.layout,
            renderer: Some(renderer),
        })
    }
}

struct Files;

impl Files {
    fn install(
        changes: impl IntoIterator<Item = Replacement>,
        store: &RecoveryStore,
        progress: &mut Progress<'_>,
        summary: &Changes,
    ) -> anyhow::Result<()> {
        let mut changed = false;
        for change in changes {
            let target = change.target().to_path_buf();
            let installed = match change.install(store) {
                Ok(installed) => installed,
                Err(error) => {
                    summary.unverified(target.clone());
                    return Err(anyhow::Error::new(error))
                        .with_context(|| format!("installing {}", target.display()));
                }
            };
            if let Some(recovery) = &installed.recovery {
                changed = true;
                progress.path("Installed:", &installed.target);
                progress.path("Recovery record:", &recovery.record);
                progress.message(format!(
                    "Inspect: omega recovery inspect {}",
                    recovery.receipt.id
                ));
            }
            summary.installed(installed);
        }
        if !changed {
            progress.skip("files already match; no replacements or backups needed");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::Ui;
    use omega_base::execution::{Pipeline, Step};
    use omega_host::{AtomicFile, Layout, TempPath, recovery::Snapshot};

    struct InstallFiles {
        files: Vec<Replacement>,
        store: RecoveryStore,
        changes: Changes,
    }

    impl Operation<()> for InstallFiles {
        type Output = ();
        type Error = anyhow::Error;

        async fn execute(self, _: (), progress: &mut Progress<'_>) -> anyhow::Result<()> {
            Files::install(self.files, &self.store, progress, &self.changes)
        }
    }

    #[tokio::test]
    async fn a_later_file_failure_keeps_confirmed_changes_and_marks_the_failed_target_unverified() {
        let root = TempPath::sibling(&std::env::temp_dir().join("omega-summary"), "test");
        let layout = Layout::at(root.join("config"), root.join("state"), root.join("cache"));
        let first = layout.workspace_manifest();
        let second = layout.system_main();
        let files = vec![
            Replacement::prepare(&first, Snapshot::file(b"first")).unwrap(),
            Replacement::prepare(&second, Snapshot::file(b"second")).unwrap(),
        ];
        AtomicFile::at(&second).write(b"external edit").unwrap();
        let changes = Changes::default();
        let pipeline =
            Pipeline::new().then(Step::new("files", "install files").using(InstallFiles {
                files,
                store: RecoveryStore::new(&layout),
                changes: changes.clone(),
            }));
        let run = pipeline.run((), &mut ()).await;
        assert!(run.result.is_err());
        let (mut ui, transcript) = Ui::recording();
        changes.show(&mut ui, &run.reports, &layout);
        let text = transcript.err();
        assert!(transcript.out().is_empty());
        assert!(
            text.contains(&format!("Created {}", first.display())),
            "{text}"
        );
        assert!(
            text.contains(&format!(
                "Could not confirm the final state of {}",
                second.display()
            )),
            "{text}"
        );
        assert!(text.contains("omega recovery restore"), "{text}");
        assert_eq!(std::fs::read(&second).unwrap(), b"external edit");
        std::fs::remove_dir_all(root).unwrap();
    }
}
