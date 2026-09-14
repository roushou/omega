//! Initialize the config workspace, daemon service, and renderer.

use crate::checkout::{CheckoutLink, SourceTree};
use crate::ui::{Paint, Step, Ui};
use crate::workspace::{ConfigWorkspace, InitialShell};
use omega_host::Layout;

/// Set omega up on this machine.
#[derive(Debug, clap::Args)]
pub struct InitCmd {
    /// Write the config workspace without installing the daemon or the
    /// renderer — for a machine that is not the one it will run on.
    #[arg(long)]
    pub bare: bool,
}

impl InitCmd {
    pub fn run(self, ui: &mut Ui) -> anyhow::Result<()> {
        let layout = Layout::resolve();
        let workspace = ConfigWorkspace::open(layout.clone())?;
        let shell = if !self.bare && !layout.system_main().exists() && layout.shell_config.exists()
        {
            InitialShell::import(&std::fs::read_to_string(&layout.shell_config)?)?
        } else {
            InitialShell::Default
        };
        let prepared = workspace.prepare_init(shell)?;
        let founded = prepared.founded;
        let plane = prepared.source_created;
        prepared.apply()?;

        if founded {
            ui.step(
                Step::Created,
                format!("{} — a Rust workspace", Paint::path(&layout.config)),
            );
        } else {
            ui.step(
                Step::Checked,
                format!("{} — already a workspace", Paint::path(&layout.config)),
            );
        }

        if plane {
            ui.step(
                Step::Created,
                format!(
                    "system/src/main.rs  {}",
                    Paint::dim("what this machine should be")
                ),
            );
        }

        // Existing configs retain their dependency-source choice on repeated init.
        if founded
            && !layout.cargo_config().exists()
            && let Some(tree) = SourceTree::detect()?
        {
            CheckoutLink::new(&workspace)
                .prepare(Some(&tree))?
                .apply()?;
            ui.step(
                Step::Linked,
                format!("building against {}", Paint::path(tree.root())),
            );
        }
        drop(workspace);

        if !self.bare {
            super::daemon::DaemonCmd::setup(ui)?;
            super::shell::ShellCmd::setup(ui)?;
        }

        ui.next("omega new <name>");
        Ok(())
    }
}
