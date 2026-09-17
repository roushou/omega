//! Run the daemon in the foreground or manage its user service.

use anyhow::Context;

use omega_daemon::Daemon;
use omega_host::Layout;
use omega_host::recovery::RecoveryStore;
use omega_platform::Brokers;
use omega_proto::Socket;

use crate::service::{DaemonService, ServiceManager};
use crate::ui::{Paint, Step, Ui};
use omega_host::systemd::{Installed, Service};

/// Run the daemon, or install it as a service.
#[derive(Debug, clap::Args)]
pub struct DaemonCmd {
    #[command(subcommand)]
    action: Option<Action>,
}

#[derive(Debug, clap::Subcommand)]
enum Action {
    /// Run it in the foreground until it is stopped. The default.
    Run,
    /// Run it at every login, and from now.
    Install(Install),
    /// Whether it is installed, whether it runs, and which omega it runs.
    Status,
    /// Stop it and take the service away again.
    Uninstall,
}

#[derive(Debug, clap::Args)]
struct Install {
    /// Set it up for the next login without starting it now.
    #[arg(long)]
    no_start: bool,
}

impl DaemonCmd {
    pub async fn run(self, ui: &mut Ui) -> anyhow::Result<()> {
        let Some(action) = self.action else {
            return Self::serve().await;
        };

        match action {
            Action::Run => Self::serve().await,
            Action::Install(install) => Self::install(install, Self::service()?, ui).await,
            Action::Status => Self::status(Self::service()?, ui).await,
            Action::Uninstall => Self::uninstall(Self::service()?, ui).await,
        }
    }

    /// Construct and run the daemon. Tracing is initialized by main.
    async fn serve() -> anyhow::Result<()> {
        let layout = Layout::resolve();
        let daemon = Daemon::from_layout(&layout)?;
        // Which brokers a daemon runs is `omega-platform`' to say, not the
        // CLI's: this is wiring, and that is the map.
        for broker in Brokers::all() {
            daemon.add_broker(broker);
        }
        daemon.run().await?;
        Ok(())
    }

    fn service() -> anyhow::Result<Service> {
        let manager = ServiceManager::detect()?.context(
            "no service manager to install into — omega knows systemd, and this machine has no user manager running",
        )?;
        Ok(manager.service(DaemonService::name())?)
    }

    async fn install(install: Install, service: Service, ui: &mut Ui) -> anyhow::Result<()> {
        let program = DaemonService::program()?;

        // A service is a promise to run this again after a reboot, and a path
        // under a build directory is a promise `cargo clean` breaks.
        if DaemonService::is_a_build_artifact(&program) {
            ui.warn(format!(
                "{} is a build directory — a service pointed there stops working when it is cleaned",
                Paint::path(&program)
            ));
        }

        let running = service.status().await?.active.is_active();
        let definition = DaemonService::definition(&program)?;
        let recovery = RecoveryStore::new(&omega_host::Layout::resolve());
        let installed = service.prepare_install(&definition)?.install(&recovery)?;
        if let Some(recovery) = &installed.recovery {
            ui.detail(format!(
                "Recovery record: {}",
                Paint::path(&recovery.record)
            ));
        } else {
            ui.detail("Installed files already match; no backup or replacement needed.");
        }
        ui.step(
            if installed.recovery.is_some() {
                Step::Installed
            } else {
                Step::Checked
            },
            format!(
                "{} — {} daemon",
                Paint::name(DaemonService::NAME),
                Paint::path(&program)
            ),
        );

        // Do not start a second daemon if a foreground process already owns the socket.
        let foreign = !install.no_start && !running && Socket::resolve().is_live();

        service.manager().reload().await.with_context(|| {
            format!(
                "inspect {}",
                service.manager().diagnose_command(Some(service.name()))
            )
        })?;
        service.enable(!install.no_start && !foreign).await?;

        // An active service must restart to use the newly installed executable.
        if running && !install.no_start {
            service.restart().await?;
        }

        if foreign {
            ui.warn(format!(
                "a daemon is already serving {} — this one takes over at the next login",
                Paint::path(Socket::resolve().path())
            ));
            ui.next("systemctl --user start omega.service");
        } else {
            ui.step(
                Step::Done,
                if install.no_start {
                    "the daemon runs at the next login"
                } else {
                    "the daemon is running, and runs at every login"
                },
            );
        }
        Ok(())
    }

    async fn status(service: Service, ui: &mut Ui) -> anyhow::Result<()> {
        let program = DaemonService::program()?;
        ui.step(
            Step::Checking,
            format!("systemd — {}", Paint::path(service.path())),
        );

        let installed = service.installed(&DaemonService::definition(&program)?)?;
        match &installed {
            Installed::Current => ui.item(
                true,
                format!(
                    "{}  {} {}",
                    Paint::name(DaemonService::NAME),
                    Paint::dim("configured for"),
                    Paint::path(&program)
                ),
            ),
            Installed::Missing => ui.item(
                false,
                format!("{}  not installed", Paint::name(DaemonService::NAME)),
            ),
            Installed::Stale { exec_start: runs } => ui.item(
                false,
                format!(
                    "{}  declares {}, and this omega is {}",
                    Paint::name(DaemonService::NAME),
                    runs.as_deref().unwrap_or("an unrecognized ExecStart"),
                    Paint::path(&program)
                ),
            ),
        }

        let status = service.status().await?;
        let enabled = status.enablement.is_persistent();
        let active = status.active.is_active();
        ui.item(
            enabled && active,
            Paint::dim(format!(
                "{}, {} ({})",
                status.enablement.as_str(),
                status.active.as_str(),
                status.sub_state
            )),
        );

        if status.needs_reload {
            ui.warn("systemd has not loaded the current unit-file contents");
        }
        if let Some(fragment) = &status.fragment
            && fragment != service.path()
        {
            ui.warn(format!(
                "systemd loaded a different unit file: {}",
                Paint::path(fragment)
            ));
        }

        // Report installed, enabled, and active states independently.
        match (
            installed == Installed::Current
                && !status.needs_reload
                && status.fragment.as_deref() == Some(service.path()),
            enabled,
            active,
        ) {
            (true, true, true) => ui.step(Step::Checked, "the daemon is part of this session"),
            (true, true, false) => ui.next("systemctl --user start omega.service"),
            _ => ui.next("omega daemon install"),
        }
        Ok(())
    }

    async fn uninstall(service: Service, ui: &mut Ui) -> anyhow::Result<()> {
        // Stop and disable the service before removing its unit file.
        if service.path().try_exists()? {
            service.disable(true).await?;
        }

        let path = service.path();
        if service.remove()? {
            ui.step(
                Step::Removed,
                format!(
                    "{} from {}",
                    Paint::name(DaemonService::NAME),
                    Paint::path(path)
                ),
            );
        } else {
            ui.step(
                Step::Done,
                format!("{} was not installed", Paint::name(DaemonService::NAME)),
            );
        }

        service.manager().reload().await.with_context(|| {
            format!(
                "inspect {}",
                service.manager().diagnose_command(Some(service.name()))
            )
        })?;
        Ok(())
    }
}
