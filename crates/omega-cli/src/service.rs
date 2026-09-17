//! Generate and manage the Omega user service.
//! The installed unit records the installing executable's absolute path.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use anyhow::Context;

use omega_host::recovery::{InstalledReplacement, RecoveryStore, Replacement, Snapshot};

/// The service that runs the daemon.
#[derive(Debug, Clone, Copy)]
pub struct Service;

impl Service {
    /// Systemd user service name.
    pub const NAME: &'static str = "omega.service";

    /// Generate a unit bound to the graphical session for the given executable.
    pub fn unit(program: &Path) -> String {
        format!(
            "\
[Unit]
Description=Omega — the desktop configuration daemon
Documentation={repository}
After=graphical-session.target
PartOf=graphical-session.target

[Service]
Type=simple
ExecStart={program} daemon
Restart=on-failure
RestartSec=1
# The daemon gives each unit five seconds to stop before it is killed, so
# systemd has to outwait that or it kills the daemon in the middle of it.
TimeoutStopSec=15

[Install]
WantedBy=graphical-session.target
",
            repository = env!("CARGO_PKG_REPOSITORY"),
            program = program.display(),
        )
    }

    /// Resolve the executable used by a new service installation.
    pub fn program() -> anyhow::Result<PathBuf> {
        std::env::current_exe().context("cannot tell where this omega is on disk")
    }

    /// Install the generated unit file as one recoverable filesystem operation.
    /// Service-manager activation remains the caller's responsibility.
    pub fn install(
        path: &Path,
        program: &Path,
        recovery: &RecoveryStore,
    ) -> anyhow::Result<InstalledReplacement> {
        Replacement::prepare(path, Snapshot::file(Self::unit(program).into_bytes()))?
            .install(recovery)
            .with_context(|| {
                format!(
                    "could not install {}; recovery records are retained",
                    path.display()
                )
            })
    }

    /// Remove the unit file. Return whether it existed.
    pub fn uninstall(path: &Path) -> anyhow::Result<bool> {
        if path.exists() {
            std::fs::remove_file(path)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Inspect the installed unit file.
    pub fn installed(path: &Path, program: &Path) -> Installed {
        let Ok(found) = std::fs::read_to_string(path) else {
            return Installed::Missing;
        };

        if found == Self::unit(program) {
            Installed::Current
        } else {
            Installed::Stale {
                program: Self::runs(&found),
            }
        }
    }

    /// Return the executable named by the installed unit's `ExecStart`.
    fn runs(unit: &str) -> Option<String> {
        let line = unit
            .lines()
            .find_map(|line| line.trim().strip_prefix("ExecStart="))?;
        Some(line.split_whitespace().next()?.to_string())
    }

    /// Whether Cargo cleanup could remove this executable.
    pub fn is_a_build_artifact(program: &Path) -> bool {
        program
            .ancestors()
            .any(|dir| dir.join("CACHEDIR.TAG").is_file())
    }
}

/// What is installed where the unit file belongs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Installed {
    /// Nothing is there.
    Missing,
    /// The unit file this binary would write, byte for byte.
    Current,
    /// An installed unit that differs from the expected contents, including its executable.
    Stale { program: Option<String> },
}

/// Supported user service manager and its unit-file location.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceManager {
    Systemd,
}

impl ServiceManager {
    /// Reload the unit, enable it, and start or restart it. A refused operation
    /// stops the sequence; service-manager success does not prove protocol health.
    pub async fn activate(self) -> anyhow::Result<()> {
        let running = self.is_active();
        self.checked(&["daemon-reload"]).await?;
        self.checked(&["enable", Service::NAME]).await?;
        self.checked(&[if running { "restart" } else { "start" }, Service::NAME])
            .await?;
        self.checked(&["is-active", Service::NAME]).await?;
        self.checked(&["is-enabled", Service::NAME]).await
    }

    async fn checked(self, args: &[&str]) -> anyhow::Result<()> {
        let output = tokio::time::timeout(
            std::time::Duration::from_secs(30),
            tokio::process::Command::new("systemctl")
                .arg("--user")
                .args(args)
                .kill_on_drop(true)
                .output(),
        )
        .await
        .context(
            "systemd operation timed out; inspect systemctl --user status omega.service before retrying",
        )?
        .with_context(|| format!("could not run systemctl --user {}; inspect {}", args.join(" "), self.diagnose_command()))?;

        anyhow::ensure!(
            output.status.success(),
            "systemd refused {} ({}): {}; inspect {}",
            args.join(" "),
            output.status,
            String::from_utf8_lossy(&output.stderr).trim(),
            self.diagnose_command()
        );

        Ok(())
    }

    /// Override the unit directory, including for isolated tests.
    pub const ENV: &'static str = "OMEGA_SERVICE_DIR";

    /// Detect a supported, running user service manager.
    pub fn detect() -> Option<Self> {
        if std::env::var_os(Self::ENV).is_some() {
            return Some(Self::Systemd);
        }

        let running = std::env::var_os("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .is_some_and(|dir| dir.join("systemd").is_dir());

        if running && which("systemctl") {
            Some(Self::Systemd)
        } else {
            None
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Systemd => "systemd",
        }
    }

    /// Where this manager reads unit files from.
    pub fn unit_dir(self) -> PathBuf {
        if let Some(named) = std::env::var_os(Self::ENV) {
            return PathBuf::from(named);
        }

        let base = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home().join(".config"));
        match self {
            Self::Systemd => base.join("systemd").join("user"),
        }
    }

    pub fn unit_path(self) -> PathBuf {
        self.unit_dir().join(Service::NAME)
    }

    /// Tell the manager to re-read what is on disk.
    pub async fn reload(self) -> anyhow::Result<()> {
        self.checked(&["daemon-reload"]).await
    }

    /// Run it at every login from now on, and — unless told otherwise — now.
    pub async fn enable(self, start: bool) -> anyhow::Result<()> {
        if start {
            self.checked(&["enable", "--now", Service::NAME]).await
        } else {
            self.checked(&["enable", Service::NAME]).await
        }
    }

    pub async fn disable(self) -> anyhow::Result<()> {
        self.checked(&["disable", "--now", Service::NAME]).await
    }

    /// Restart the service to activate updated unit-file contents.
    pub async fn restart(self) -> anyhow::Result<()> {
        self.checked(&["restart", Service::NAME]).await
    }

    /// Whether it is set to start at login.
    pub fn is_enabled(self) -> bool {
        self.succeeds(&["is-enabled", Service::NAME])
    }

    /// Whether it is running right now.
    pub fn is_active(self) -> bool {
        self.succeeds(&["is-active", Service::NAME])
    }

    /// Diagnostic command for service-manager failures.
    pub fn diagnose_command(self) -> String {
        match self {
            Self::Systemd => format!("systemctl --user status {}", Service::NAME),
        }
    }

    fn succeeds(self, args: &[&str]) -> bool {
        self.run(args)
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    fn run(self, args: &[&str]) -> std::io::Result<Output> {
        match self {
            Self::Systemd => Command::new("systemctl").arg("--user").args(args).output(),
        }
    }
}

fn which(program: &str) -> bool {
    std::env::var_os("PATH")
        .is_some_and(|path| std::env::split_paths(&path).any(|dir| dir.join(program).is_file()))
}

fn home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"))
}
