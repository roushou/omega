//! Restore a validated generation through the ordinary activation path.

use crate::ui::{Paint, Step, Ui};
use omega_daemon::reconcile::ValidatedBuild;
use omega_host::{GenerationId, Generations, Layout};

#[derive(Debug, clap::Args)]
pub struct RollbackCmd {
    /// Restore this generation; otherwise restore acceptance or its predecessor.
    pub generation: Option<GenerationId>,
}

impl RollbackCmd {
    pub fn run(self, ui: &mut Ui) -> anyhow::Result<()> {
        let store = Generations::new(&Layout::resolve());
        let rollback = store.rollback(self.generation.as_ref())?;
        ValidatedBuild::read(rollback.generation().clone())?;
        let generation = rollback.commit()?;
        ui.step(Step::Restored, Paint::path(&generation.layout().state));
        ui.step(
            Step::Next,
            "the daemon adopts this generation through normal convergence",
        );
        Ok(())
    }
}
