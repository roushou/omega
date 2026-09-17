//! Omega's daemon definition and user-session discovery policy.
//! Generic unit files and manager operations live in `omega_host::systemd`.

use anyhow::Context;
use omega_host::{
    Layout,
    systemd::{ExecStart, Manager, Restart, Scope, Service, ServiceUnit, UnitName},
};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

/// The daemon's session binding, executable, and activation policy.
#[derive(Debug)]
pub struct DaemonService;

impl DaemonService {
    pub const NAME: &'static str = "omega.service";

    pub fn name() -> UnitName {
        UnitName::parse(Self::NAME).expect("bundled service name")
    }

    pub fn definition(program: &Path) -> anyhow::Result<ServiceUnit> {
        let session = UnitName::parse("graphical-session.target")?;
        Ok(ServiceUnit::new(ExecStart::new(program)?.arg("daemon")?)
            .description("Omega — the desktop configuration daemon")?
            .documentation(env!("CARGO_PKG_REPOSITORY"))?
            .after(session.clone())
            .part_of(session.clone())
            .wanted_by(session)
            .restart(Restart::OnFailure, Duration::from_secs(1))
            // Outwait the daemon's five-second plugin shutdown grace.
            .stop_timeout(Duration::from_secs(15)))
    }

    pub fn program() -> anyhow::Result<PathBuf> {
        std::env::current_exe().context("cannot tell where this omega is on disk")
    }

    pub fn is_a_build_artifact(program: &Path) -> bool {
        program
            .ancestors()
            .any(|dir| dir.join("CACHEDIR.TAG").is_file())
    }

    /// Reload, enable, and start or restart in order. Check systemd state here;
    /// initialization separately verifies the daemon's protocol readiness.
    pub(crate) async fn activate(service: &Service) -> anyhow::Result<()> {
        let running = service.status().await?.active.is_active();
        service.manager().reload().await.with_context(|| {
            format!(
                "inspect {}",
                service.manager().diagnose_command(Some(service.name()))
            )
        })?;
        service.enable(false).await?;
        if running {
            service.restart().await?;
        } else {
            service.start().await?;
        }

        let status = service.status().await?;
        anyhow::ensure!(
            status.active.is_active() && status.enablement.is_persistent(),
            "daemon service is {}, {}; inspect {}",
            status.active.as_str(),
            status.enablement.as_str(),
            service.manager().diagnose_command(Some(service.name()))
        );
        Ok(())
    }
}

/// Resolve the user-session manager and unit-file directory once at the CLI boundary.
#[derive(Debug, Clone)]
pub struct ServiceManager {
    manager: Manager,
    directory: PathBuf,
}

impl ServiceManager {
    pub const ENV: &'static str = "OMEGA_SERVICE_DIR";

    pub fn detect() -> anyhow::Result<Option<Self>> {
        let override_dir = std::env::var_os(Self::ENV).map(PathBuf::from);
        let running = std::env::var_os("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .is_some_and(|dir| dir.join("systemd").is_dir());
        if override_dir.is_none() && !running {
            return Ok(None);
        }

        let directory = match override_dir {
            Some(directory) => directory,
            None => {
                let config = match std::env::var_os("XDG_CONFIG_HOME") {
                    Some(path) if !path.is_empty() => PathBuf::from(path),
                    _ => PathBuf::from(
                        std::env::var_os("HOME")
                            .context("HOME is required to locate user service files")?,
                    )
                    .join(".config"),
                };
                config.join("systemd/user")
            }
        };
        anyhow::ensure!(
            directory.is_absolute(),
            "user service directory must be absolute: {}",
            directory.display()
        );
        Ok(Some(Self {
            manager: Manager::new(Scope::User),
            directory,
        }))
    }

    pub fn service(&self, name: UnitName) -> Result<Service, omega_host::systemd::ServiceError> {
        let path = Layout::service_file(&self.directory, &name);
        Service::new(self.manager.clone(), name, path)
    }
}
