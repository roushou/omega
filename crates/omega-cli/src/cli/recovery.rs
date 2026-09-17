//! Explicit inspection and recovery of retained filesystem changes.

use crate::{
    ui::{Paint, Step, Ui},
    workspace::ConfigWorkspace,
};
use anyhow::ensure;
use omega_host::{
    Layout,
    recovery::{Change, ChangeId, RecoveryStore, Replacement},
};

/// Inspect retained installation records or recover one filesystem change.
#[derive(Debug, clap::Args)]
pub struct RecoveryCmd {
    #[command(subcommand)]
    action: Action,
}

#[derive(Debug, clap::Subcommand)]
enum Action {
    /// List retained change IDs, kinds, and journal states.
    List,
    /// Show the recorded target and its current relationship to the backup.
    Inspect {
        #[arg(value_name = "ID")]
        change_id: ChangeId,
    },
    /// Confirm an interrupted write that already reached its intended state.
    Accept {
        #[arg(value_name = "ID")]
        change_id: ChangeId,
    },
    /// Restore one target, refusing subsequent edits. Stop the daemon first.
    Restore {
        #[arg(value_name = "ID")]
        change_id: ChangeId,
    },
}

impl RecoveryCmd {
    pub fn run(self, ui: &mut Ui) -> anyhow::Result<()> {
        let layout = Layout::resolve();
        let store = RecoveryStore::new(&layout);

        if matches!(self.action, Action::List) {
            if !layout.recovery_dir().try_exists()? {
                ui.line("No retained changes.");
                return Ok(());
            }
            for receipt in store.receipts()? {
                ui.line(format!(
                    "{}\t{:?}\t{}",
                    receipt.id, receipt.state, receipt.kind
                ));
            }
            return Ok(());
        }

        let change_id = match &self.action {
            Action::Inspect { change_id }
            | Action::Accept { change_id }
            | Action::Restore { change_id } => change_id,
            Action::List => unreachable!("handled above"),
        };

        // Match source mutation lock order: workspace, then recovery store.
        let _workspace = if matches!(self.action, Action::Restore { .. }) {
            ensure!(
                !omega_proto::Socket::resolve().is_live(),
                "stop the daemon before restoring files: systemctl --user stop omega.service (or stop the foreground daemon)"
            );
            Some(ConfigWorkspace::open(layout.clone())?)
        } else {
            None
        };

        let mut saved = store.open::<Replacement>(change_id)?;
        match self.action {
            Action::Inspect { .. } => {
                ui.line(format!(
                    "Record: {}\nKind: {}\nTarget: {}\nJournal: {:?}\nObserved: {:?}",
                    saved.path().display(),
                    Replacement::KIND,
                    saved.change().target().display(),
                    saved.receipt().state,
                    saved.inspect()?
                ));
            }
            Action::Accept { .. } => {
                saved.accept()?;
                ui.step(Step::Checked, "observed change confirmed durable");
            }
            Action::Restore { .. } => {
                saved.restore()?;
                ui.step(Step::Restored, Paint::path(saved.change().target()));
                ui.detail("Only this target was restored. Service enablement, running processes, and published generations are unchanged.");
                ui.detail("For a service file, run systemctl --user daemon-reload. For renderer assets, restart Omarchy. Restore source edits in reverse order; rebuild before starting the daemon again.");
            }
            Action::List => unreachable!("handled above"),
        }
        Ok(())
    }
}
