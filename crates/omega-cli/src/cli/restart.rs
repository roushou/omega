//! `omega restart`: cycle a plugin without touching its neighbours.

use omega_proto::PluginName;

use crate::operator::Operator;
use crate::ui::{Paint, Step, Ui};

/// Restart one plugin. The document still says it should run; this asks only
/// that it stop being this instance of it.
#[derive(Debug, clap::Args)]
pub struct RestartCmd {
    #[arg(value_name = "PLUGIN")]
    pub plugin_name: PluginName,
}

impl RestartCmd {
    pub async fn run(self, ui: &mut Ui) -> anyhow::Result<()> {
        let plugin_name = self.plugin_name;
        Operator::new().restart(&plugin_name).await?;
        ui.step(Step::Restarted, Paint::name(&plugin_name));
        Ok(())
    }
}
