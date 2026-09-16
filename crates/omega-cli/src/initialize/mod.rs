//! Initialization declares one pipeline; operations own effects and return data.

mod pipeline;
mod preflight;
mod steps;

use crate::{
    renderer::Verification,
    ui::{Paint, PipelineResult, Step, Ui},
};
use omega_host::{Layout, Profile};
use omega_proto::Socket;

#[derive(Clone)]
pub(crate) struct Initialize {
    pub(crate) layout: Layout,
    pub(crate) bare: bool,
    pub(crate) profile: Profile,
    pub(crate) socket: Socket,
}

impl Initialize {
    pub(crate) async fn run(self, ui: &mut Ui) -> anyhow::Result<()> {
        let pipeline = pipeline::Steps::production().compose(self.bare);
        let recovery_dir = self.layout.recovery_dir();
        let run = pipeline.run(self, ui).await;
        match PipelineResult::finish(run.result) {
            Ok(report) => {
                report.show(ui);
                Ok(())
            }
            Err(error) => {
                ui.detail("Completed changes are retained. Fix the reported problem, then rerun omega init; unchanged files will be kept.");
                ui.detail(format!(
                    "File recovery records: {}",
                    Paint::path(recovery_dir)
                ));
                ui.next("omega recovery list");
                ui.detail("Inspect a pending record before retrying: omega recovery inspect <id>; accept a completed write or restore it with omega recovery accept|restore <id>.");
                Err(error.context("initialization stopped"))
            }
        }
    }
}

pub(super) struct InitReport {
    layout: Layout,
    renderer: Option<Verification>,
    backup: Option<std::path::PathBuf>,
}

impl InitReport {
    fn show(self, ui: &mut Ui) {
        match self.renderer {
            None => ui.step(
                Step::Done,
                "workspace initialized; --bare skips compilation and desktop setup",
            ),
            Some(renderer) => {
                renderer.show(ui);
                ui.step(
                    Step::Done,
                    "configuration built and applied; daemon running and enabled at login",
                );
                ui.detail(format!(
                    "Shell configuration: {}",
                    Paint::path(&self.layout.shell_config)
                ));
            }
        }
        ui.detail(format!("Workspace: {}", Paint::path(&self.layout.config)));
        if let Some(backup) = self.backup {
            ui.detail(format!("Original shell backup: {}", Paint::path(backup)));
            ui.detail("To recover the original shell: stop omega.service, restore this backup over shell.json, then restart Omarchy. Keep the daemon stopped until your Rust layout agrees.");
        }
        ui.next("omega new <name>");
    }
}

#[cfg(test)]
mod tests;
