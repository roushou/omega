use super::Initialize;
use crate::{
    checkout::SourceTree,
    service::{DaemonService, ServiceManager},
    workspace::{ConfigWorkspace, InitialShell},
};
use anyhow::{Context, ensure};
use omega_base::execution::{Operation, Progress};
use omega_host::recovery::{RecoveryStore, Replacement};
use omega_host::systemd::{Service, ServiceUnit};
use omega_omarchy::{
    HostShell, Renderer,
    installation::{InstallationState, ShellInstallation},
};
use std::time::Duration;

pub(super) struct Preflight;
pub(super) struct PrepareWorkspace;

pub(super) struct Inspected {
    pub(super) request: Initialize,
    pub(super) source: Option<SourceTree>,
    pub(super) host: Option<HostPlan>,
}

pub(super) struct Prepared {
    pub(super) request: Initialize,
    pub(super) workspace: ConfigWorkspace,
    pub(super) files: Vec<Replacement>,
    pub(super) imported: Option<serde_json::Value>,
    pub(super) source: Option<SourceTree>,
    pub(super) host: Option<HostPlan>,
}

pub(super) struct HostPlan {
    pub(super) host: Host,
    pub(super) service: Replacement,
    pub(super) renderers: Vec<Replacement>,
}

pub(super) struct Host {
    pub(super) service: Service,
    pub(super) definition: ServiceUnit,
    pub(super) shell: HostShell,
}

impl Operation<Initialize> for Preflight {
    type Output = Inspected;
    type Error = anyhow::Error;

    async fn execute(
        self,
        request: Initialize,
        progress: &mut Progress<'_>,
    ) -> anyhow::Result<Inspected> {
        progress.path("Workspace:", &request.layout.config);
        RecoveryStore::new(&request.layout).check_pending()?;

        let host = if request.bare {
            None
        } else {
            for program in ["cargo", "rustc"] {
                Self::probe(program, &["--version"])
                    .await
                    .with_context(|| {
                        format!("{program} is required to build your Rust configuration")
                    })?;
            }

            let manager = ServiceManager::detect()?.context(
                "no systemd user manager; use omega init --bare to create only the workspace",
            )?;
            let service = manager.service(DaemonService::name())?;
            service.manager().probe().await.context(
                "the systemd user manager is unavailable; use omega init --bare for workspace-only setup",
            )?;
            ensure!(
                !request.socket.is_live() || service.status().await?.active.is_active(),
                "a foreground daemon owns {}; stop it before initialization to avoid starting a second daemon",
                request.socket.path().display()
            );

            let shell = HostShell::detect().context(
                "no supported host shell; use omega init --bare to create only the workspace",
            )?;
            Self::probe("omarchy", &["--help"])
                .await
                .context(
                    "Omarchy is required to restart the shell; use omega init --bare for workspace-only setup",
                )?;

            let program = DaemonService::program()?;
            let definition = DaemonService::definition(&program)?;
            let service_file = service.prepare_install(&definition)?;
            let renderers = Renderer::ALL
                .iter()
                .map(|renderer| renderer.prepare(&shell.plugins()))
                .collect::<anyhow::Result<_>>()?;

            Some(HostPlan {
                host: Host {
                    service,
                    definition,
                    shell,
                },
                service: service_file,
                renderers,
            })
        };

        let source = if !request.layout.workspace_manifest().try_exists()?
            && !request.layout.cargo_config().try_exists()?
        {
            let source = SourceTree::detect()?;
            if let Some(source) = &source {
                source.version()?;
                source.patch(false)?;
            }
            source
        } else {
            None
        };

        Ok(Inspected {
            request,
            source,
            host,
        })
    }
}

impl Operation<Inspected> for PrepareWorkspace {
    type Output = Prepared;
    type Error = anyhow::Error;

    async fn execute(self, inspected: Inspected, _: &mut Progress<'_>) -> anyhow::Result<Prepared> {
        let Inspected {
            request,
            source,
            host,
        } = inspected;
        let workspace = ConfigWorkspace::open(request.layout.clone())?;
        let layout = workspace.layout();

        let shell = if !request.bare
            && !layout.system_main().try_exists()?
            && layout.shell_config.try_exists()?
        {
            InitialShell::import(&std::fs::read_to_string(&layout.shell_config)?)?
        } else {
            InitialShell::Default
        };
        let config = workspace.prepare_init(shell)?;
        let imported = config.imported().cloned();

        if !request.bare && imported.is_none() {
            match ShellInstallation::new(layout).inspect()? {
                InstallationState::ModifiedExternally => anyhow::bail!(
                    "shell configuration has external edits; run omega shell diff before initializing again"
                ),
                InstallationState::Unmanaged if layout.shell_config.try_exists()? => anyhow::bail!(
                    "existing shell configuration is unmanaged; run omega shell adopt before initializing this workspace"
                ),
                InstallationState::Unmanaged
                | InstallationState::Current
                | InstallationState::Missing => {}
            }
        }

        let files = config.replacements()?;
        drop(config);

        Ok(Prepared {
            request,
            workspace,
            files,
            imported,
            source,
            host,
        })
    }
}

impl Preflight {
    async fn probe(program: &str, args: &[&str]) -> anyhow::Result<()> {
        let output = tokio::time::timeout(
            Duration::from_secs(10),
            tokio::process::Command::new(program)
                .args(args)
                .kill_on_drop(true)
                .output(),
        )
        .await
        .with_context(|| format!("{program} did not respond within 10s"))??;

        ensure!(
            output.status.success(),
            "{program} is unavailable: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );

        Ok(())
    }
}
