//! Initialization declares one pipeline; operations own effects and return data.

mod diagnostic;
mod pipeline;
mod preflight;
mod steps;
mod summary;

use crate::{
    renderer::Verification,
    ui::{Paint, Step, Ui},
};
use omega_host::{Layout, Profile};
use omega_proto::Socket;

pub(crate) use diagnostic::InitFailure;
use summary::Changes;

#[derive(Clone)]
pub(crate) struct Initialize {
    pub(crate) layout: Layout,
    pub(crate) bare: bool,
    pub(crate) profile: Profile,
    pub(crate) socket: Socket,
}

impl Initialize {
    pub(crate) async fn run(self, ui: &mut Ui) -> anyhow::Result<()> {
        let changes = Changes::default();
        let pipeline = pipeline::Steps::production(&changes).compose(self.bare);
        let run = pipeline.run(self.clone(), ui).await;

        changes.show(ui, &run.reports, &self.layout);

        match run.result {
            Ok(report) => {
                report.show(ui);
                Ok(())
            }
            Err(failure) => Err(InitFailure::new(failure, &run.reports, &self).into()),
        }
    }
}

pub(super) struct InitReport {
    layout: Layout,
    renderer: Option<Verification>,
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
        ui.next("omega new <name>");
    }
}

#[cfg(test)]
mod tests;
