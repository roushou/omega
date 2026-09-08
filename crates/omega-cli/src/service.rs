//! Making the daemon part of the session.
//!
//! Run from a terminal, omega is something you started. Run as a service, it
//! is part of the desktop: it comes back after a reboot, it is restarted when
//! it fails, and it goes away with the session that it draws into.
//!
//! Unlike the renderer, the unit file cannot be carried in the binary — it
//! names the binary's own path, so it is written from whichever `omega` is
//! doing the installing. That is also the failure worth diagnosing: two omegas
//! on one machine, with the service running the other one.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use anyhow::Context;

use omega_proto::AtomicFile;

/// The service that runs the daemon.
#[derive(Debug, Clone, Copy)]
pub struct Service;

impl Service {
    /// What the unit is called, which is what a person types after
    /// `systemctl --user`.
    pub const NAME: &'static str = "omega.service";

    /// The unit file for a daemon run from `program`.
    ///
    /// `graphical-session.target` on both sides is the whole of the lifetime:
    /// the daemon binds its sockets in `$XDG_RUNTIME_DIR` and draws through a
    /// shell that belongs to one session, so it starts with a session and is
    /// stopped with it rather than lingering as a daemon with nothing to draw
    /// into.
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

    /// The omega a service installed now would run.
    pub fn program() -> anyhow::Result<PathBuf> {
        std::env::current_exe().context("cannot tell where this omega is on disk")
    }

    /// Write the unit file at `path`, replacing whatever was there.
    ///
    /// The path is given rather than asked of a [`ServiceManager`]: which
    /// file this is belongs to the manager, and what goes in it does not.
    pub fn install(path: &Path, program: &Path) -> anyhow::Result<()> {
        AtomicFile::at(path)
            .write(Self::unit(program).as_bytes())
            .with_context(|| format!("could not write {}", path.display()))?;
        Ok(())
    }

    /// Take the unit file away. Says whether there was one.
    pub fn uninstall(path: &Path) -> anyhow::Result<bool> {
        if path.exists() {
            std::fs::remove_file(path)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// What is on disk where the unit file belongs.
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

    /// The omega an installed unit file runs, which is the diagnostic that
    /// matters: a service that came back after a reboot running a different
    /// binary looks exactly like one that did not come back at all.
    fn runs(unit: &str) -> Option<String> {
        let line = unit
            .lines()
            .find_map(|line| line.trim().strip_prefix("ExecStart="))?;
        Some(line.split_whitespace().next()?.to_string())
    }

    /// Whether this program sits in a directory cargo builds into, and so is
    /// one `cargo clean` away from a service that cannot start.
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
    /// Something else — an older install, a hand edit, or one that runs a
    /// different omega. The program is the one it runs, when it names one.
    Stale { program: Option<String> },
}

/// What keeps a user's services running on this machine.
///
/// A type rather than a path, for the same reason [`omega_renderer::HostShell`]
/// is one: installing is more than writing a file — something has to be told
/// the file exists, and told to run it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceManager {
    Systemd,
}

impl ServiceManager {
    /// Names the unit directory outright, for a machine laid out unusually —
    /// and for the tests, which must not write where a real manager reads.
    pub const ENV: &'static str = "OMEGA_SERVICE_DIR";

    /// The service manager on this machine, if omega knows how to install
    /// into it.
    ///
    /// Both halves matter: `systemctl` on a machine whose user manager is not
    /// running would take an install and never run it.
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
    pub fn reload(self) -> std::io::Result<Output> {
        self.run(&["daemon-reload"])
    }

    /// Run it at every login from now on, and — unless told otherwise — now.
    pub fn enable(self, start: bool) -> std::io::Result<Output> {
        if start {
            self.run(&["enable", "--now", Service::NAME])
        } else {
            self.run(&["enable", Service::NAME])
        }
    }

    pub fn disable(self) -> std::io::Result<Output> {
        self.run(&["disable", "--now", Service::NAME])
    }

    /// Run it again, so a rewritten unit file is the one in effect.
    ///
    /// Enabling an already-running service starts nothing: the instance that
    /// is up keeps the `ExecStart` it was started with, which after an
    /// install is the previous omega.
    pub fn restart(self) -> std::io::Result<Output> {
        self.run(&["restart", Service::NAME])
    }

    /// Whether it is set to start at login.
    pub fn is_enabled(self) -> bool {
        self.succeeds(&["is-enabled", Service::NAME])
    }

    /// Whether it is running right now.
    pub fn is_active(self) -> bool {
        self.succeeds(&["is-active", Service::NAME])
    }

    /// How a person reads what went wrong, which omega does not paraphrase.
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
