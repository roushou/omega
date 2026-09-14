//! Parse check requests without owning compilation or validation.
use crate::{build::Check, ui::Ui};
use omega_host::Layout;

/// Compile and validate the configuration without publishing a generation.
#[derive(Debug, clap::Args)]
pub struct CheckCmd;
impl CheckCmd {
    pub async fn run(self, ui: &mut Ui) -> anyhow::Result<()> {
        Check {
            layout: Layout::resolve(),
        }
        .run(ui)
        .await
    }
}
