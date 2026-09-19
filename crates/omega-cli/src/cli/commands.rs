//! Inspect registered command contracts, provider availability, and execution diagnostics.
use crate::{operator::Operator, ui::Ui};
#[derive(Debug, clap::Args)]
pub struct CommandsCmd {
    /// Limit the catalogue to one plugin or command host.
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
        catalogue.hosts.retain(|host| {
            self.plugin
                .as_ref()
                .is_none_or(|plugin| host.id == plugin.as_str())
        });
        if self.json {
            ui.line(serde_json::to_string_pretty(&catalogue)?);
        } else {
            ui.command_hosts(&catalogue.hosts, self.plugin.is_some());
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
