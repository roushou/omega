//! Run the daemon in the foreground or manage its user service.

use anyhow::Context;

use omega_daemon::Daemon;
use omega_host::Layout;
use omega_host::recovery::RecoveryStore;
use omega_platform::Brokers;
use omega_proto::Socket;

use crate::service::{Installed, Service, ServiceManager};
use crate::ui::{Paint, Step, Ui};

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
            Action::Install(install) => Self::install(install, Self::manager()?, ui),
            Action::Status => Self::status(Self::manager()?, ui),
            Action::Uninstall => Self::uninstall(Self::manager()?, ui),
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

    fn manager() -> anyhow::Result<ServiceManager> {
        ServiceManager::detect().context(
            "no service manager to install into — omega knows systemd, and this machine has no user manager running",
        )
    }

    fn install(install: Install, manager: ServiceManager, ui: &mut Ui) -> anyhow::Result<()> {
        let program = Service::program()?;

        // A service is a promise to run this again after a reboot, and a path
        // under a build directory is a promise `cargo clean` breaks.
        if Service::is_a_build_artifact(&program) {
            ui.warn(format!(
                "{} is a build directory — a service pointed there stops working when it is cleaned",
                Paint::path(&program)
            ));
        }

        let recovery = RecoveryStore::new(&omega_host::Layout::resolve());
        let installed = Service::install(&manager.unit_path(), &program, &recovery)?;
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
                Paint::name(Service::NAME),
                Paint::path(&program)
            ),
        );

        // Do not start a second daemon if a foreground process already owns the socket.
        let running = manager.is_active();
        let foreign = !install.no_start && !running && Socket::resolve().is_live();

        // Stop installation reporting if the service manager rejects an operation.
        if !Self::tell(manager.reload(), manager, ui)?
            || !Self::tell(manager.enable(!install.no_start && !foreign), manager, ui)?
        {
            return Ok(());
        }

        // An active service must restart to use the newly installed executable.
        if running && !install.no_start && !Self::tell(manager.restart(), manager, ui)? {
            return Ok(());
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

    fn status(manager: ServiceManager, ui: &mut Ui) -> anyhow::Result<()> {
        let program = Service::program()?;
        ui.step(
            Step::Checking,
            format!("{} — {}", manager.name(), Paint::path(manager.unit_path())),
        );

        let installed = Service::installed(&manager.unit_path(), &program);
        match &installed {
            Installed::Current => ui.item(
                true,
                format!(
                    "{}  {} {}",
                    Paint::name(Service::NAME),
                    Paint::dim("runs"),
                    Paint::path(&program)
                ),
            ),
            Installed::Missing => ui.item(
                false,
                format!("{}  not installed", Paint::name(Service::NAME)),
            ),
            Installed::Stale { program: runs } => ui.item(
                false,
                format!(
                    "{}  runs {}, and this omega is {}",
                    Paint::name(Service::NAME),
                    runs.as_deref().unwrap_or("something else"),
                    Paint::path(&program)
                ),
            ),
        }

        // Query enablement only for an installed service.
        let (enabled, active) = match installed {
            Installed::Missing => (false, false),
            _ => (manager.is_enabled(), manager.is_active()),
        };

        if installed != Installed::Missing {
            ui.item(
                enabled && active,
                Paint::dim(format!(
                    "{}, {}",
                    if enabled {
                        "starts at login"
                    } else {
                        "does not start at login"
                    },
                    if active { "running now" } else { "not running" }
                )),
            );
        }

        // Report installed, enabled, and active states independently.
        match (installed == Installed::Current, enabled, active) {
            (true, true, true) => ui.step(Step::Checked, "the daemon is part of this session"),
            (true, true, false) => ui.next("systemctl --user start omega.service"),
            _ => ui.next("omega daemon install"),
        }
        Ok(())
    }

    fn uninstall(manager: ServiceManager, ui: &mut Ui) -> anyhow::Result<()> {
        // Stop and disable the service before removing its unit file.
        if manager.unit_path().exists() {
            Self::tell(manager.disable(), manager, ui)?;
        }

        let path = manager.unit_path();
        if Service::uninstall(&path)? {
            ui.step(
                Step::Removed,
                format!("{} from {}", Paint::name(Service::NAME), Paint::path(path)),
            );
        } else {
            ui.step(
                Step::Done,
                format!("{} was not installed", Paint::name(Service::NAME)),
            );
        }

        Self::tell(manager.reload(), manager, ui)?;
        Ok(())
    }

    /// Report service-manager stderr when an operation fails.
    fn tell(
        result: std::io::Result<std::process::Output>,
        manager: ServiceManager,
        ui: &mut Ui,
    ) -> anyhow::Result<bool> {
        let output = result.with_context(|| format!("could not run {}", manager.name()))?;
        if output.status.success() {
            return Ok(true);
        }

        let said = String::from_utf8_lossy(&output.stderr);
        ui.warn(format!(
            "{} refused: {}",
            manager.name(),
            said.trim().lines().next().unwrap_or("no reason given")
        ));
        ui.next(&manager.diagnose_command());
        Ok(false)
    }
}
