//! Inspect registered command contracts and connection availability.
use crate::{operator::Operator, ui::Ui};
#[derive(Debug, clap::Args)]
pub struct CommandsCmd {
    /// Limit the catalogue to one plugin.
    pub plugin: Option<omega_proto::PluginName>,
    /// Emit full input/output contracts as JSON.
    #[arg(long)]
    pub json: bool,
}
impl CommandsCmd {
    pub async fn run(self, ui: &mut Ui) -> anyhow::Result<()> {
        let mut catalogue = Operator::new().commands().await?;
        catalogue.entries.retain(|entry| {
            self.plugin
                .as_ref()
                .is_none_or(|plugin| entry.plugin == plugin.as_str())
        });
        if self.json {
            ui.line(serde_json::to_string_pretty(&catalogue)?);
        } else {
            for entry in catalogue.entries {
                let endpoint = entry
                    .endpoint
                    .ok_or_else(|| anyhow::anyhow!("catalogue entry has no descriptor"))?;
                ui.line(format!(
                    "{}::{}\t{}\t{}",
                    entry.plugin,
                    endpoint.id,
                    if entry.available {
                        "available"
                    } else {
                        "offline"
                    },
                    endpoint.description
                ));
            }
        }
        Ok(())
    }
}
