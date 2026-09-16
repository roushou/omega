use super::{
    InitReport, Initialize,
    preflight::{Host, HostPlan, Prepared},
};
use crate::{
    build::{
        activation::Activation,
        steps::{Published, Sources, Validated},
    },
    checkout::CheckoutLink,
    operator::Operator,
    renderer::{RendererStatus, Snapshot},
    service::{Installed, Service},
    workspace::ConfigWorkspace,
};
use anyhow::{Context, ensure};
use omega_base::execution::{Operation, Progress};
use omega_host::recovery::{RecoveryStore, Replacement};
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

pub(super) struct AdoptShell;

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
            ShellInstallation::new(layout).adopt(expected)?;
            progress.path("Original shell backup:", layout.shell_backup());
            progress.path("Shell ownership receipt:", layout.shell_receipt());
        } else {
            progress.skip("no shell import to adopt");
        }
        Ok(prepared)
    }
}

pub(super) struct WriteWorkspace;

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
        )?;
        Ok(prepared)
    }
}

pub(super) struct ConfigureDependencies;

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
            backup: None,
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

pub(super) struct InstallRenderer;

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

pub(super) struct InstallService;

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
            !runtime.socket.is_live() || runtime.host.manager.is_active(),
            "a foreground daemon appeared during initialization; stop it before retrying"
        );
        runtime.host.manager.activate().await?;
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
            Service::installed(&runtime.host.unit_path, &runtime.host.program)
                == Installed::Current,
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
        let backup = applied.published.layout.shell_backup();
        let backup = backup.try_exists()?.then_some(backup);
        Ok(InitReport {
            layout: applied.published.layout,
            renderer: Some(renderer),
            backup,
        })
    }
}

struct Files;

impl Files {
    fn install(
        changes: impl IntoIterator<Item = Replacement>,
        store: &RecoveryStore,
        progress: &mut Progress<'_>,
    ) -> anyhow::Result<()> {
        let mut changed = false;
        for change in changes {
            let installed = change.install(store)?;
            if let Some(recovery) = installed.recovery {
                changed = true;
                progress.path("Updated:", installed.target);
                progress.path("Recovery record:", recovery.record);
                progress.message(format!(
                    "Inspect: omega recovery inspect {}",
                    recovery.receipt.id
                ));
            }
        }
        if !changed {
            progress.skip("files already match; no replacements or backups needed");
        }
        Ok(())
    }
}
