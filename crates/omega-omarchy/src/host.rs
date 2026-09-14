//! The shell that draws plugins on this machine.

use std::path::PathBuf;
use std::process::Output;

/// Omarchy host discovery, plugin paths, and installation commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostShell {
    /// Omarchy's Quickshell process, which hosts the bar.
    Omarchy,
}

impl HostShell {
    /// Names the plugin directory outright, for a machine laid out unusually
    /// — and for the tests, which have no shell to find.
    pub const ENV: &'static str = "OMEGA_SHELL_PLUGINS";

    /// The shell on this machine, if omega knows how to install into it.
    pub fn detect() -> Option<Self> {
        if std::env::var_os(Self::ENV).is_some() || Self::Omarchy.config_dir().is_dir() {
            Some(Self::Omarchy)
        } else {
            None
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Omarchy => "Omarchy",
        }
    }

    /// Where this shell looks for plugins.
    pub fn plugins(self) -> PathBuf {
        if let Some(named) = std::env::var_os(Self::ENV) {
            return PathBuf::from(named);
        }
        match self {
            Self::Omarchy => self.config_dir().join("plugins"),
        }
    }

    /// Request a shell plugin rescan. Installation remains valid if the shell is unavailable.
    pub fn rescan(self) -> std::io::Result<Output> {
        match self {
            Self::Omarchy => std::process::Command::new("omarchy-shell")
                .args(["shell", "rescanPlugins"])
                .output(),
        }
    }

    /// Restart the host process to discard cached QML components.
    pub fn restart_command(self) -> std::process::Command {
        match self {
            Self::Omarchy => {
                let mut command = std::process::Command::new("omarchy");
                command.args(["restart", "shell"]);
                command
            }
        }
    }

    /// Enable the widget in the host layout when explicitly requested.
    pub fn enable(self, id: &str) -> std::io::Result<Output> {
        match self {
            Self::Omarchy => std::process::Command::new("omarchy")
                .args(["plugin", "enable", id])
                .output(),
        }
    }

    /// Return the command for enabling the widget in the host layout.
    pub fn enable_command(self, id: &str) -> String {
        match self {
            Self::Omarchy => format!("omarchy plugin enable {id} --section right"),
        }
    }

    /// Return the shell reload command for applying linked renderer edits.
    pub fn reload_command(self) -> &'static str {
        match self {
            Self::Omarchy => "omarchy restart shell",
        }
    }

    fn config_dir(self) -> PathBuf {
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home().join(".config"));
        match self {
            Self::Omarchy => base.join("omarchy"),
        }
    }
}

fn home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"))
}
