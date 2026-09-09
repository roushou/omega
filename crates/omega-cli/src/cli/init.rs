//! `omega init`: make this machine run omega.
//!
//! Everything a machine needs and nothing about any particular plugin: the
//! workspace a config lives in, the service that keeps the daemon running,
//! and the renderer that draws what plugins publish. Writing a plugin is
//! `omega new`, and it is a different day's work — which is why founding a
//! config used to require naming one, and why that was wrong.
//!
//! Each step is the same code the standalone command runs, reported on its
//! own line, so what this did stays visible and individually re-runnable.

use omega_daemon::host::cargo::{CargoManifest, CargoSlot};
use omega_host::{AtomicFile, Layout};

use crate::cli::link::LinkCmd;
use crate::scaffold::Scaffold;
use crate::ui::{Paint, Step, Ui};

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
        let scaffold = Scaffold::new();

        // Never rewritten: they are the author's files once they exist, so a
        // second `omega init` sets the machine up again and leaves the config
        // alone.
        let workspace = layout.file::<CargoManifest>(CargoSlot::Workspace);
        let founded = workspace.create_new(&scaffold.workspace_manifest())?;

        let system = layout.file::<CargoManifest>(CargoSlot::System);
        let plane = system.create_new(&scaffold.system_manifest())?;
        if plane {
            AtomicFile::at(layout.system_main()).write(scaffold.system_main().as_bytes())?;
        }

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

        // A first config on a machine that has an omega checkout is built
        // against it: before there is anything published, that is every
        // machine, and it is what makes the scaffold work out of the box.
        if let Some(tree) = LinkCmd::on_init(&layout)? {
            ui.step(
                Step::Linked,
                format!("building against {}", Paint::path(tree.root())),
            );
        }

        if !self.bare {
            super::daemon::DaemonCmd::setup(ui)?;
            super::shell::ShellCmd::setup(ui)?;
        }

        ui.next("omega new <name>");
        Ok(())
    }
}
