//! `omega restart`: cycle a unit without touching its neighbours.

use omega_proto::UnitName;

use crate::operator::Operator;
use crate::ui::{Paint, Step, Ui};

/// Restart one unit. The document still says it should run; this asks only
/// that it stop being this instance of it.
#[derive(Debug, clap::Args)]
pub struct RestartCmd {
    #[arg(value_name = "UNIT")]
    pub unit_name: UnitName,
}

impl RestartCmd {
    pub async fn run(self, ui: &mut Ui) -> anyhow::Result<()> {
        let unit_name = self.unit_name;
        Operator::new().restart(&unit_name).await?;
        ui.step(Step::Restarted, Paint::name(&unit_name));
        Ok(())
    }
}
