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

use crate::checkout::{CheckoutLink, SourceTree};
use crate::workspace::ConfigWorkspace;
use omega_daemon::host::cargo::CargoConfig;
use omega_host::Layout;

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
                Paint::command("omega init")
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
            None => SourceTree::detect()?.with_context(|| {
                format!(
                    "no omega checkout to link — name one, or set {}",
                    Paint::name(SourceTree::ENV)
                )
            })?,
        };

        let workspace = ConfigWorkspace::open(layout.clone())?;
        CheckoutLink::new(&workspace)
            .prepare(Some(&tree))?
            .apply()?;

        // A patch only applies to a requirement it satisfies, so linking also
        // makes the config ask for the version the checkout carries. Without
        // this, a checkout that has moved on leaves cargo reporting a patch
        // that "was not used" and no way to see why.
        let version = tree.version()?;

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
        let workspace = ConfigWorkspace::open(layout.clone())?;
        CheckoutLink::new(&workspace).prepare(None)?.apply()?;
        ui.step(
            Step::Linked,
            format!(
                "{} — building against the published crates",
                Paint::path(&layout.config)
            ),
        );
        Ok(())
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
