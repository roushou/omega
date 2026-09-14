//! The shell that draws plugins on this machine.

use std::path::PathBuf;
use std::process::Output;

/// The shell that draws plugins on this machine.
///
/// A type rather than a path, because installing is more than copying: each
/// shell has its own plugin directory, its own way of being told to look
/// again, and its own words for putting a widget on screen.
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

    /// Tell the shell to look again.
    ///
    /// Belt to the shell's own braces: it reloads when a file under its
    /// plugin directory changes, so a shell that was not watching — or was
    /// not running — is not a failed install, and this is allowed to fail.
    pub fn rescan(self) -> std::io::Result<Output> {
        match self {
            Self::Omarchy => std::process::Command::new("omarchy-shell")
                .args(["shell", "rescanPlugins"])
                .output(),
        }
    }

    /// Put the widget on screen. Only ever on request: `omega shell install`
    /// prints the command instead, because rearranging somebody's bar unasked
    /// is how a command stops being trusted. `omega init` is the exception.
    pub fn enable(self, id: &str) -> std::io::Result<Output> {
        match self {
            Self::Omarchy => std::process::Command::new("omarchy")
                .args(["plugin", "enable", id])
                .output(),
        }
    }

    /// How a person puts the widget on screen, which omega does not do for
    /// them: where a widget sits in the bar is their layout, not ours.
    pub fn enable_command(self, id: &str) -> String {
        match self {
            Self::Omarchy => format!("omarchy plugin enable {id} --section right"),
        }
    }

    /// How a person makes the shell pick up changed plugin code.
    ///
    /// For a linked install, which the shell finds but does not watch: a
    /// rescan re-reads manifests, and the widget already on screen keeps
    /// running the code it was built with until the shell restarts.
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
