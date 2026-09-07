//! `omega daemon`: run the daemon, or make it part of the session.
//!
//! Bare, it runs in the foreground — the way a person watches it while they
//! are working on something. Installed, it is a service the session manager
//! keeps running, which is the difference between a process somebody started
//! and a desktop that comes back after a reboot.
//!
//! One noun on purpose. The unit file is not the daemon, and the type that
//! writes it says so ([`crate::service`]) — but from outside there is one
//! background process, and making somebody learn a second word for it to find
//! out how to keep it running serves the implementation, not them.

use anyhow::Context;

use omega_core::Layout;
use omega_daemon::Daemon;
use omega_daemon::sources::Battery;
use omega_wire::Socket;

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
        // Running is what `omega daemon` has always meant, and it is what a
        // person types most; the service verbs join it rather than displace it.
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

    /// Run the daemon against `~/.local/state/omega`.
    ///
    /// Tracing is initialized once in `main`; this only wires the daemon
    /// together and runs it.
    async fn serve() -> anyhow::Result<()> {
        let layout = Layout::resolve();
        let daemon = Daemon::from_layout(&layout)?;
        daemon.add_source(Battery::new());
        daemon.run().await?;
        Ok(())
    }

    fn manager() -> anyhow::Result<ServiceManager> {
        ServiceManager::detect().context(
            "no service manager to install into — omega knows systemd, and this machine has no user manager running",
        )
    }

    /// Install the service as part of setting omega up.
    ///
    /// A machine with no service manager is not a failed setup: the daemon
    /// still runs, it just has to be started by hand.
    pub(crate) fn setup(ui: &mut Ui) -> anyhow::Result<()> {
        let Some(manager) = ServiceManager::detect() else {
            ui.warn("no service manager here — start the daemon with `omega daemon`");
            return Ok(());
        };
        Self::install(Install { no_start: false }, manager, ui)
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

        Service::install(&manager.unit_path(), &program)?;
        ui.step(
            Step::Installed,
            format!(
                "{} — {} daemon",
                Paint::name(Service::NAME),
                Paint::path(&program)
            ),
        );

        // Whoever holds the socket decides what happens next. This service
        // already holding it is a reinstall; anything else holding it is a
        // daemon somebody started, and starting a second one over it produces
        // a failed service and an error about a path that says nothing about
        // the one they are running.
        let running = manager.is_active();
        let foreign = !install.no_start && !running && Socket::resolve().is_live();

        // A manager that refused has already said why, in its own words. It
        // must not then be told that the daemon is running.
        if !Self::tell(manager.reload(), manager, ui)?
            || !Self::tell(manager.enable(!install.no_start && !foreign), manager, ui)?
        {
            return Ok(());
        }

        // Enabling a service that is already up starts nothing, so the
        // instance still running is the one started from the unit file this
        // install just replaced.
        if running && !install.no_start && !Self::tell(manager.restart(), manager, ui)? {
            return Ok(());
        }

        match foreign {
            true => {
                ui.warn(format!(
                    "a daemon is already serving {} — this one takes over at the next login",
                    Paint::path(Socket::resolve().path())
                ));
                ui.next("systemctl --user start omega.service");
            }
            false => ui.step(
                Step::Done,
                match install.no_start {
                    true => "the daemon runs at the next login",
                    false => "the daemon is running, and runs at every login",
                },
            ),
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
            // The failure that looks like every other failure: the service
            // came back after a reboot, and came back as somebody else.
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

        // Asked of the manager only when there is a unit for it to know
        // about: `is-enabled` on a unit that does not exist is an error
        // report, not an answer.
        let (enabled, active) = match installed {
            Installed::Missing => (false, false),
            _ => (manager.is_enabled(), manager.is_active()),
        };

        if installed != Installed::Missing {
            ui.item(
                enabled && active,
                Paint::dim(format!(
                    "{}, {}",
                    match enabled {
                        true => "starts at login",
                        false => "does not start at login",
                    },
                    match active {
                        true => "running now",
                        false => "not running",
                    }
                )),
            );
        }

        // Installed and enabled is not the same as running, and a summary
        // that says a stopped daemon is part of the session is the one thing
        // worse than saying nothing.
        match (installed == Installed::Current, enabled, active) {
            (true, true, true) => ui.step(Step::Checked, "the daemon is part of this session"),
            (true, true, false) => ui.next("systemctl --user start omega.service"),
            _ => ui.next("omega daemon install"),
        }
        Ok(())
    }

    fn uninstall(manager: ServiceManager, ui: &mut Ui) -> anyhow::Result<()> {
        // Stopped before the file goes: a manager asked to disable a unit it
        // can no longer read leaves it running with nothing describing it.
        // And only when there is one — asking systemd to disable a unit that
        // was never installed is an error report about nothing.
        if manager.unit_path().exists() {
            Self::tell(manager.disable(), manager, ui)?;
        }

        let path = manager.unit_path();
        match Service::uninstall(&path)? {
            true => ui.step(
                Step::Removed,
                format!("{} from {}", Paint::name(Service::NAME), Paint::path(path)),
            ),
            false => ui.step(
                Step::Done,
                format!("{} was not installed", Paint::name(Service::NAME)),
            ),
        }

        Self::tell(manager.reload(), manager, ui)?;
        Ok(())
    }

    /// Report what the manager said, and only when it refused.
    ///
    /// Its own words rather than a paraphrase: whatever systemd has to say
    /// about a unit it would not take is more use than anything omega could
    /// say on its behalf.
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
