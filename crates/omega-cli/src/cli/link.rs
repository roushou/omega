//! Configure machine-local Cargo patches for an Omega checkout.
//! Overrides live in the gitignored `.cargo/config.toml`; manifest dependencies
//! continue to name published packages.

use anyhow::{Context, bail};

use crate::checkout::{CheckoutLink, SourceTree};
use crate::workspace::ConfigWorkspace;
use omega_host::Layout;

use crate::ui::{Paint, Step, Ui};

/// Point this config at a checkout of omega, or back at the published crates.
#[derive(Debug, clap::Args)]
pub struct LinkCmd {
    /// The checkout to build against. Defaults to `$OMEGA_SOURCE`, else the
    /// tree this binary was built from.
    pub path: Option<std::path::PathBuf>,

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

        // Cargo ignores patches whose versions do not satisfy dependency requirements.
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
}
