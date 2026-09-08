//! What omega carries for the shell to draw with, and how it gets there.
//!
//! The files travel *inside* the binary. A renderer and the daemon are one
//! protocol in two halves and have to be the same version, so the copy a
//! binary installs is by construction the one its daemon speaks to — a
//! stale copy stops being a mistake to be careful about and becomes a state
//! that cannot be reached.

use std::path::{Path, PathBuf};

use anyhow::{Context, bail};

use omega_proto::StageDir;

use crate::installed::Installed;

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
            source: concat!("crates/omega-renderer/", $dir),
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
            "Connection.qml",
            "ViewNode.qml",
            "Props.js",
            "Icons.js",
            "nodes/Text.qml",
            "nodes/Icon.qml",
            "nodes/Progress.qml",
            "nodes/Button.qml",
            "nodes/Slider.qml",
            "nodes/Toggle.qml",
            "nodes/Field.qml",
            "nodes/List.qml",
            "nodes/Stack.qml",
            "nodes/Separator.qml",
            "nodes/Spacer.qml",
            "nodes/Header.qml",
            "nodes/Graph.qml",
            "nodes/Group.qml",
            "nodes/Grid.qml",
            "nodes/Image.qml"
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
        if differs || self.has_strays(&dir) {
            Installed::Stale {
                version: self.installed_version(&dir),
            }
        } else {
            Installed::Current
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

    /// Anything under the directory that this renderer does not carry.
    ///
    /// A walk, not a listing: the tree has a `nodes/` directory in it, and a
    /// check that stopped at the top would read every install as stale — and
    /// would miss a stray file left inside it by an older version.
    fn has_strays(&self, dir: &Path) -> bool {
        let Some(found) = Self::walk(dir) else {
            return true;
        };
        found
            .iter()
            .any(|name| !self.files.iter().any(|asset| asset.name == name))
    }

    /// Every file under `dir`, named the way an [`Asset`] is: relative, with
    /// `/` between the parts. `None` when the directory cannot be read.
    pub fn walk(dir: &Path) -> Option<Vec<String>> {
        fn descend(root: &Path, dir: &Path, found: &mut Vec<String>) -> Option<()> {
            for entry in std::fs::read_dir(dir).ok()?.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    descend(root, &path, found)?;
                } else {
                    found.push(
                        path.strip_prefix(root)
                            .ok()?
                            .to_string_lossy()
                            .replace(std::path::MAIN_SEPARATOR, "/"),
                    );
                }
            }
            Some(())
        }

        let mut found = Vec::new();
        descend(dir, dir, &mut found)?;
        found.sort();
        Some(found)
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
