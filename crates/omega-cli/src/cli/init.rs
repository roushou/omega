//! Initialize the workspace and verify its desktop activation.

use crate::{initialize::Initialize, ui::Ui};
use omega_host::{Layout, Profile};
use omega_proto::Socket;

/// Set Omega up on this machine.
#[derive(Debug, clap::Args)]
pub struct InitCmd {
    /// Create only the workspace, without compiling or changing desktop startup.
    #[arg(long)]
    pub bare: bool,
    /// Compile using the debug profile without optimizations.
    #[arg(long, conflicts_with = "bare")]
    pub debug: bool,
}

impl InitCmd {
    pub async fn run(self, ui: &mut Ui) -> anyhow::Result<()> {
        Initialize {
            layout: Layout::resolve(),
            bare: self.bare,
            profile: if self.debug {
                Profile::Debug
            } else {
                Profile::Release
            },
            socket: Socket::resolve(),
        }
        .run(ui)
        .await
    }
}
