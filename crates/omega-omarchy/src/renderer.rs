//! Embedded Omarchy renderer assets and installation.
//! Installation writes the files carried by this binary; status compares installed
//! versions and contents with those embedded files.

use std::path::{Path, PathBuf};

use anyhow::{Context, bail};

use omega_host::{
    StageDir,
    recovery::{InstalledReplacement, RecoveryStore, Replacement, Snapshot},
};

use crate::installed::Installed;

use omega_renderer::{Asset, Core};

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

/// Embed assets using crate-local paths so packaged builds retain every file.
/// Checkout-relative paths also support linked installations.
macro_rules! renderer {
    ($id:literal, $dir:literal, [$($name:literal),* $(,)?]) => {
        Renderer {
            id: $id,
            source: concat!("crates/omega-omarchy/", $dir),
            files: &[$(Asset {
                name: $name,
                // Relative to this file: `src` → the crate root.
                contents: include_str!(concat!("../", $dir, "/", $name)),
            }),*],
        }
    };
}

impl Renderer {
    /// Version required by the embedded renderer manifest.
    pub const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    /// Draws the view tree a plugin publishes, as a widget in the bar.
    pub const VIEW: Renderer = renderer!(
        "omega.view",
        "shell",
        [
            "manifest.json",
            "BarWidget.qml",
            "Connection.qml",
            "PlacementConnection.qml",
            "PlacementConnections.qml",
            "qmldir",
            "PanelSession.qml",
            "OmarchyTheme.qml"
        ]
    );

    pub fn assets(&self) -> impl Iterator<Item = &Asset> {
        self.files.iter().chain(Core::FILES)
    }

    /// All Omarchy renderer plugins installed by this integration.
    pub const ALL: &'static [Renderer] = &[Self::VIEW];

    /// Where this renderer belongs under a shell's plugin directory.
    pub fn dir_in(&self, plugins: &Path) -> PathBuf {
        plugins.join(self.id)
    }

    /// A file this renderer carries.
    pub fn file(&self, name: &str) -> Option<&'static str> {
        self.assets()
            .find(|asset| asset.name == name)
            .map(|asset| asset.contents)
    }

    /// Read the embedded manifest version, verified against [`Renderer::VERSION`] by tests.
    pub fn declared_version(&self) -> Option<String> {
        declared(self.file("manifest.json")?, "version")
    }

    /// Identity of every asset in this renderer, including shared controls.
    pub fn build(&self) -> omega_renderer::Build {
        omega_renderer::Build::of(Self::VERSION, self.assets())
    }

    /// Atomically replace the installed asset directory and remove obsolete assets.
    pub fn install(
        &self,
        plugins: &Path,
        recovery: &RecoveryStore,
    ) -> anyhow::Result<InstalledReplacement> {
        let dir = self.dir_in(plugins);
        self.prepare(plugins)?.install(recovery).with_context(|| {
            format!(
                "could not install into {}; recovery records are retained",
                dir.display()
            )
        })
    }

    /// Capture the existing renderer and embedded replacement without writing
    /// either. Publication rechecks this snapshot before changing the directory.
    pub fn prepare(&self, plugins: &Path) -> anyhow::Result<Replacement> {
        let dir = self.dir_in(plugins);
        let build = self.build();
        let desired = Snapshot::directory(
            self.assets()
                .map(|asset| (asset.name, build.contents(asset).as_bytes().to_vec())),
        )?;
        Ok(Replacement::prepare(&dir, desired)?)
    }

    /// Link a source checkout for renderer development.
    /// File watchers do not traverse the link; apply edits with the host reload command.
    pub fn link(&self, plugins: &Path, checkout: &Path) -> anyhow::Result<PathBuf> {
        let source = checkout.join(self.source);
        if !source.join("manifest.json").is_file()
            || !checkout.join(Core::SOURCE).join("ViewNode.qml").is_file()
        {
            bail!("{} is not a renderer", source.display());
        }

        let dir = self.dir_in(plugins);
        omega_host::Directory::create_all(plugins)
            .with_context(|| format!("could not create {}", plugins.display()))?;
        let stage = StageDir::new(&dir)?;
        for asset in self.files {
            std::os::unix::fs::symlink(source.join(asset.name), stage.path().join(asset.name))?;
        }
        std::os::unix::fs::symlink(checkout.join(Core::SOURCE), stage.path().join("core"))?;
        stage.write(".omega-link", source.to_string_lossy().as_bytes())?;
        stage.commit()?;
        Ok(dir)
    }

    /// Remove this renderer's installation. Refuse directories identifying another plugin.
    pub fn uninstall(&self, plugins: &Path) -> anyhow::Result<Option<PathBuf>> {
        let dir = self.dir_in(plugins);
        if !dir.is_symlink() && !dir.exists() {
            return Ok(None);
        }

        // Stale installations retain this renderer's identity and can be removed.
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

        if let Ok(source) = std::fs::read_to_string(dir.join(".omega-link")) {
            return Installed::Linked(source.into());
        }
        if dir.is_symlink() {
            return match std::fs::read_link(&dir) {
                Ok(target) => Installed::Linked(target),
                Err(_) => Installed::Stale { version: None },
            };
        }
        if !dir.is_dir() {
            return Installed::Missing;
        }

        let build = self.build();
        let differs = self.assets().any(|asset| {
            std::fs::read_to_string(dir.join(asset.name))
                .ok()
                .as_deref()
                != Some(build.contents(asset).as_ref())
        });

        // Extra files, including nested files, make an installation differ.
        if differs || self.has_strays(&dir) {
            Installed::Stale {
                version: self.installed_version(&dir),
            }
        } else {
            Installed::Current
        }
    }

    /// Read the installed manifest's declared version.
    fn installed_version(&self, dir: &Path) -> Option<String> {
        let manifest = std::fs::read_to_string(dir.join("manifest.json")).ok()?;
        declared(&manifest, "version")
    }

    /// Verify the installed directory identifies this renderer.
    fn is_ours(&self, dir: &Path) -> bool {
        let declared_id = std::fs::read_to_string(dir.join("manifest.json"))
            .ok()
            .and_then(|manifest| declared(&manifest, "id"));
        declared_id.as_deref() == Some(self.id)
    }

    /// Find files absent from the embedded asset list, recursively.
    fn has_strays(&self, dir: &Path) -> bool {
        let Some(found) = Self::walk(dir) else {
            return true;
        };
        found
            .iter()
            .any(|name| !self.assets().any(|asset| asset.name == name))
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

/// Read one top-level string field from a plugin manifest.
fn declared(manifest: &str, field: &str) -> Option<String> {
    let parsed: serde_json::Value = serde_json::from_str(manifest).ok()?;
    Some(parsed.get(field)?.as_str()?.to_owned())
}
