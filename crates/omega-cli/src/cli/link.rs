//! `omega link`: build this config against a checkout of omega.
//!
//! A config's `Cargo.toml` names the published crates, because a config is a
//! git repository that has to build on every machine it is cloned onto. That
//! is the right thing to commit and the wrong thing for somebody working on
//! omega's internals, who needs their own tree.
//!
//! Both are true at once through cargo's own answer: `[patch]`, written to
//! `.cargo/config.toml`, which the scaffold tells git to ignore. The manifest
//! stays portable; this machine builds against a checkout; and nobody's home
//! directory ends up in a committed file.

use anyhow::{Context, bail};

use omega_daemon::host::cargo::{CargoConfig, CargoManifest, CargoSlot, Dependencies, Dependency};
use omega_proto::{AtomicFile, Layout};

use crate::scaffold::SourceTree;
use crate::ui::{Paint, Step, Ui};

/// Point this config at a checkout of omega, or back at the published crates.
#[derive(Debug, clap::Args)]
pub struct LinkCmd {
    /// The checkout to build against. Defaults to `$OMEGA_SOURCE`, else the
    /// tree this binary was built from.
    pub path: Option<String>,

    /// Undo it: build against the published crates, as a clone of this config
    /// would on any other machine.
    #[arg(long, conflicts_with = "path")]
    pub published: bool,
}

impl LinkCmd {
    pub async fn run(self, ui: &mut Ui) -> anyhow::Result<()> {
        let layout = Layout::resolve();
        if !layout.workspace_manifest().exists() {
            bail!(
                "{} is not a Rust workspace — start one with {}",
                Paint::path(&layout.config),
                Paint::command("omega init <name>")
            );
        }

        if self.published {
            Self::unlink(&layout, ui)
        } else {
            self.link(&layout, ui)
        }
    }

    fn link(self, layout: &Layout, ui: &mut Ui) -> anyhow::Result<()> {
        let tree = match self.path {
            Some(path) => SourceTree::at(path)?,
            None => SourceTree::detect().with_context(|| {
                format!(
                    "no omega checkout to link — name one, or set {}",
                    Paint::name(SourceTree::ENV)
                )
            })?,
        };

        Self::write_patch(layout, tree.patch()?)?;

        // A patch only applies to a requirement it satisfies, so linking also
        // makes the config ask for the version the checkout carries. Without
        // this, a checkout that has moved on leaves cargo reporting a patch
        // that "was not used" and no way to see why.
        let version = tree.version()?;
        Self::require(layout, &version)?;

        ui.step(
            Step::Linked,
            format!(
                "{} — omega {version} from {}",
                Paint::path(&layout.config),
                Paint::path(tree.root())
            ),
        );
        Ok(())
    }

    fn unlink(layout: &Layout, ui: &mut Ui) -> anyhow::Result<()> {
        Self::write_patch(layout, Dependencies::new())?;
        ui.step(
            Step::Linked,
            format!(
                "{} — building against the published crates",
                Paint::path(&layout.config)
            ),
        );
        Ok(())
    }

    /// Replace the patch table, leaving anything else in the file alone: it
    /// is cargo's config, not omega's, and somebody may have put their own
    /// settings in it.
    fn write_patch(layout: &Layout, patched: Dependencies) -> anyhow::Result<()> {
        let file = layout.file::<CargoConfig>(());
        let mut config = file.read_or_default()?;
        config.replace_patch(patched);

        match config.is_empty() {
            // Nothing left to say. A file that says nothing is a file that
            // invites the question of what it is for.
            true if file.path().exists() => Ok(std::fs::remove_file(file.path())?),
            true => Ok(()),
            false => Ok(file.write(&config)?),
        }
    }

    /// Ask for the version the checkout carries.
    fn require(layout: &Layout, version: &str) -> anyhow::Result<()> {
        let file = layout.file::<CargoManifest>(CargoSlot::Workspace);
        file.edit(|manifest| {
            let Some(workspace) = manifest.workspace.as_mut() else {
                return;
            };
            for spec in crate::scaffold::Scaffold::omega_crates() {
                if workspace.dependencies.contains(spec.name) {
                    let updated = match spec.package {
                        None => Dependency::registry(version, &[]),
                        Some(package) => Dependency::renamed(package, version, &[]),
                    };
                    workspace.dependencies.insert(spec.name, updated);
                }
            }
        })?;
        Ok(())
    }

    /// What `omega init` writes when it can see a checkout: the same patch,
    /// so a contributor's first config builds without a second command.
    pub fn on_init(layout: &Layout) -> anyhow::Result<Option<SourceTree>> {
        // True whether or not there is a checkout to link: the build output
        // and this machine's opinion of where omega lives are never a config
        // repository's business.
        AtomicFile::at(layout.gitignore())
            .write(crate::scaffold::Scaffold::new().gitignore().as_bytes())?;

        let Some(tree) = SourceTree::detect() else {
            return Ok(None);
        };
        Self::write_patch(layout, tree.patch()?)?;
        Ok(Some(tree))
    }

    /// Why a build might have failed to resolve omega at all.
    ///
    /// A config that is not linked and asks for crates nobody has published
    /// fails in cargo's words, which name a package and say nothing about
    /// omega. This is the sentence that was missing.
    pub fn unlinked(layout: &Layout) -> Option<String> {
        let linked = layout
            .file::<CargoConfig>(())
            .read_or_default()
            .ok()
            .and_then(|config| config.patched().map(|patched| !patched.is_empty()))
            .unwrap_or(false);

        if linked {
            None
        } else {
            Some(format!(
                "this config is not linked to a checkout of omega, and asks for crates that may not be published yet — point it at one with {}",
                Paint::command("omega link <path>")
            ))
        }
    }
}
