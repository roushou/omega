//! The renderers omega ships, and the host shell they install into.
//!
//! A unit publishes a view tree; something has to draw it. That something is
//! QML running inside the host shell's own long-lived process, and it reads
//! the same wire format the daemon writes — protobuf's `intValue` arriving as
//! a *string* is one fact with a reader on each side of it.
//!
//! So a renderer and the daemon are one protocol in two halves, and they have
//! to be the same version. That is why the QML lives in this repository and
//! travels *inside* the binary rather than being copied out of a checkout:
//! the renderer a binary installs is, by construction, the one its daemon
//! speaks to. A stale copy stops being a mistake to be careful about and
//! becomes a state that cannot be reached.

use std::path::{Path, PathBuf};
use std::process::Output;

use anyhow::{Context, bail};

use omega_daemon::host::StageDir;

/// One file of a renderer, carried in the binary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Asset {
    pub name: &'static str,
    pub contents: &'static str,
}

/// A plugin omega ships for the host shell to draw.
#[derive(Debug, Clone, Copy)]
pub struct Renderer {
    /// The plugin id, which is also the directory it installs into.
    pub id: &'static str,
    /// Where it lives in a checkout — for `--link`, and for the test that
    /// catches a file added to the tree and left out of `files`.
    pub source: &'static str,
    /// Every file the plugin is made of.
    pub files: &'static [Asset],
}

/// Declare a renderer from files read out of the tree at compile time, so the
/// list and the contents cannot disagree. The path is used twice: relative to
/// this file for the compiler, relative to a checkout for `--link`. It must
/// stay inside this crate — cargo packages a crate's own directory and
/// nothing above it.
macro_rules! renderer {
    ($id:literal, $dir:literal, [$($name:literal),* $(,)?]) => {
        Renderer {
            id: $id,
            source: concat!("crates/omega-cli/", $dir),
            files: &[$(Asset {
                name: $name,
                // Relative to this file: `src` → the crate root.
                contents: include_str!(concat!("../", $dir, "/", $name)),
            }),*],
        }
    };
}

impl Renderer {
    /// The version every renderer carries, which is omega's own: what they
    /// read is what this binary writes.
    pub const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    /// Draws the view tree a unit publishes, as a widget in the bar.
    pub const VIEW: Renderer = renderer!(
        "omega.view",
        "shell/plugins/omega.view",
        [
            "manifest.json",
            "BarWidget.qml",
            "ViewNode.qml",
            "Props.js",
            "Icons.js"
        ]
    );

    /// Every renderer omega ships.
    ///
    /// One today. A surface other than the bar — an OSD, a full-screen
    /// overlay — would be a second, installed by the same command, because
    /// what they all have to stay in step with is the same daemon.
    pub const ALL: &'static [Renderer] = &[Self::VIEW];

    /// Where this renderer belongs under a shell's plugin directory.
    pub fn dir_in(&self, plugins: &Path) -> PathBuf {
        plugins.join(self.id)
    }

    /// A file this renderer carries.
    pub fn file(&self, name: &str) -> Option<&'static str> {
        self.files
            .iter()
            .find(|asset| asset.name == name)
            .map(|asset| asset.contents)
    }

    /// The version the carried manifest announces to the shell.
    ///
    /// Pinned to [`Renderer::VERSION`] by a test rather than patched at
    /// install time: the file that ships is the file that is written, so the
    /// copy in the repository is the copy a `--link` install serves, and
    /// there is no third version of the truth.
    pub fn declared_version(&self) -> Option<String> {
        declared(self.file("manifest.json")?, "version")
    }

    /// Write the files this binary carries, replacing whatever was there.
    ///
    /// Through a staging directory for two reasons: the shell watches this
    /// tree and reloads on any change, so it must never see it half-written;
    /// and the swap is what removes a file an older renderer shipped, which a
    /// copy on top of a copy would leave behind to be loaded forever.
    pub fn install(&self, plugins: &Path) -> anyhow::Result<PathBuf> {
        let dir = self.dir_in(plugins);
        std::fs::create_dir_all(plugins)
            .with_context(|| format!("could not create {}", plugins.display()))?;

        // A previous `--link` is a symlink, and swapping a directory in over
        // one leaves the link behind under the staging backup's name.
        if dir.is_symlink() {
            std::fs::remove_file(&dir)?;
        }

        let stage = StageDir::new(&dir)?;
        for asset in self.files {
            stage.write(asset.name, asset.contents.as_bytes())?;
        }
        stage
            .commit()
            .with_context(|| format!("could not install into {}", dir.display()))?;
        Ok(dir)
    }

    /// Point the shell at a checkout instead of copying it, for editing the
    /// QML without reinstalling. The deviation, not the default: the shell
    /// then draws whatever is in that tree.
    ///
    /// A linked plugin is found by the scan but not *watched* — inotify does
    /// not descend through a symlink — so edits need
    /// [`HostShell::reload_command`].
    pub fn link(&self, plugins: &Path, checkout: &Path) -> anyhow::Result<PathBuf> {
        let source = checkout.join(self.source);
        if !source.join("manifest.json").is_file() {
            bail!("{} is not a renderer", source.display());
        }

        let dir = self.dir_in(plugins);
        std::fs::create_dir_all(plugins)
            .with_context(|| format!("could not create {}", plugins.display()))?;
        self.clear(&dir)?;

        std::os::unix::fs::symlink(&source, &dir)
            .with_context(|| format!("could not link {}", dir.display()))?;
        Ok(dir)
    }

    /// Remove an installed renderer, and nothing else.
    ///
    /// A directory under the shell's plugins that is not this renderer is
    /// somebody else's plugin — possibly one they wrote — so it is left alone
    /// and said so, rather than deleted because the name matched.
    pub fn uninstall(&self, plugins: &Path) -> anyhow::Result<Option<PathBuf>> {
        let dir = self.dir_in(plugins);
        if !dir.is_symlink() && !dir.exists() {
            return Ok(None);
        }

        // Only a copy that is not this renderer's is somebody else's; a
        // stale one is still ours, and is exactly what wants removing.
        let unrecognised =
            matches!(self.installed(plugins), Installed::Stale { .. }) && !self.is_ours(&dir);
        if unrecognised {
            bail!(
                "{} does not declare {} — refusing to remove somebody else's plugin",
                dir.display(),
                self.id
            );
        }

        self.clear(&dir)?;
        Ok(Some(dir))
    }

    /// What is on disk where this renderer belongs.
    pub fn installed(&self, plugins: &Path) -> Installed {
        let dir = self.dir_in(plugins);

        if dir.is_symlink() {
            return match std::fs::read_link(&dir) {
                Ok(target) => Installed::Linked(target),
                Err(_) => Installed::Stale { version: None },
            };
        }
        if !dir.is_dir() {
            return Installed::Missing;
        }

        let differs = self.files.iter().any(|asset| {
            std::fs::read_to_string(dir.join(asset.name))
                .ok()
                .as_deref()
                != Some(asset.contents)
        });

        // A file an older renderer shipped, or one somebody added, is a
        // difference too: the shell loads the directory, not our list.
        match differs || self.has_strays(&dir) {
            true => Installed::Stale {
                version: self.installed_version(&dir),
            },
            false => Installed::Current,
        }
    }

    /// The version an installed copy announces, for saying what is there.
    fn installed_version(&self, dir: &Path) -> Option<String> {
        let manifest = std::fs::read_to_string(dir.join("manifest.json")).ok()?;
        declared(&manifest, "version")
    }

    /// Whether an installed copy is this renderer at all, rather than another
    /// plugin that happens to sit under the same name.
    fn is_ours(&self, dir: &Path) -> bool {
        let declared_id = std::fs::read_to_string(dir.join("manifest.json"))
            .ok()
            .and_then(|manifest| declared(&manifest, "id"));
        declared_id.as_deref() == Some(self.id)
    }

    /// Anything in the directory that this renderer does not carry.
    fn has_strays(&self, dir: &Path) -> bool {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return true;
        };
        entries.flatten().any(|entry| {
            let name = entry.file_name();
            !self
                .files
                .iter()
                .any(|asset| name.as_os_str() == asset.name)
        })
    }

    /// Take away whatever is at `dir`, link or directory.
    fn clear(&self, dir: &Path) -> anyhow::Result<()> {
        match dir.is_symlink() {
            true => std::fs::remove_file(dir)?,
            false if dir.exists() => std::fs::remove_dir_all(dir)?,
            false => {}
        }
        Ok(())
    }
}

/// One top-level string field of a plugin manifest.
///
/// Enough of the manifest to say which plugin it is and which version — the
/// shell owns the rest of the schema, and parsing more of it here would be
/// omega holding an opinion about a file it only writes.
fn declared(manifest: &str, field: &str) -> Option<String> {
    let parsed: serde_json::Value = serde_json::from_str(manifest).ok()?;
    Some(parsed.get(field)?.as_str()?.to_owned())
}

/// What is installed where a renderer belongs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Installed {
    /// Nothing is there.
    Missing,
    /// A symlink to a checkout: somebody is working on the QML, and what the
    /// shell draws is whatever is in their tree.
    Linked(PathBuf),
    /// The files this binary carries, byte for byte.
    Current,
    /// Something else — an older install, or a hand edit. The version is what
    /// the copy on disk claims, when it claims one.
    Stale { version: Option<String> },
}

impl Installed {
    /// How this differs from what the binary carries, when it does — the one
    /// phrase `omega shell status` and `omega check` both report.
    ///
    /// The case worth spelling out is a copy that claims the right version
    /// and holds different bytes: "0.1.0 installed, this omega draws 0.1.0"
    /// reads as a bug in the check rather than a fact about the disk. The
    /// manifest is what a copy says of itself; the contents are the evidence.
    pub fn difference(&self) -> Option<String> {
        let Self::Stale { version } = self else {
            return None;
        };
        Some(match version.as_deref() {
            Some(found) if found == Renderer::VERSION => "edited since it was installed".to_owned(),
            Some(found) => format!("{found} installed, this omega draws {}", Renderer::VERSION),
            None => "no manifest omega can read".to_owned(),
        })
    }
}

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
        match std::env::var_os(Self::ENV).is_some() || Self::Omarchy.config_dir().is_dir() {
            true => Some(Self::Omarchy),
            false => None,
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
