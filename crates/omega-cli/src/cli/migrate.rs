use crate::{
    ui::{Paint, Step, Ui},
    workspace::ConfigWorkspace,
};
use omega_host::Layout;

/// Migrate config sources from units/ to plugins/, preserving package identities.
#[derive(Debug, clap::Args)]
pub struct MigrateCmd {
    /// Inspect and report the migration without changing source files.
    #[arg(long)]
    check: bool,
}

impl MigrateCmd {
    pub fn run(self, ui: &mut Ui) -> anyhow::Result<()> {
        let workspace = ConfigWorkspace::open(Layout::resolve())?;
        if !self.check && workspace.recover_migration()? {
            ui.step(Step::Restored, "interrupted workspace migration");
        }
        let Some(migration) = workspace.prepare_migration()? else {
            ui.step(Step::Checked, "workspace already uses the current layout");
            return Ok(());
        };
        ui.step(Step::Checking, "units/ → plugins/");
        for path in migration.paths() {
            ui.step(Step::Changed, Paint::path(path));
        }
        if self.check {
            ui.next("omega migrate");
        } else {
            migration.apply(&workspace)?;
            ui.step(Step::Done, "workspace migrated to plugins/");
            ui.next("omega check");
        }
        Ok(())
    }
}
