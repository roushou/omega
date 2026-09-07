//! `omega restart`: cycle a unit without touching its neighbours.

use omega_core::UnitName;

use crate::operator::Operator;
use crate::ui::{Paint, Step, Ui};

/// Restart one unit. The document still says it should run; this asks only
/// that it stop being this instance of it.
#[derive(Debug, clap::Args)]
pub struct RestartCmd {
    pub unit: String,
}

impl RestartCmd {
    pub async fn run(self, ui: &mut Ui) -> anyhow::Result<()> {
        let name = UnitName::parse(&self.unit)?;
        Operator::new().restart(name.as_str()).await?;
        ui.step(Step::Restarted, Paint::name(&name));
        Ok(())
    }
}
