//! Compile, validate, and publish config generations without owning CLI syntax.
use crate::{
    build::steps::{Sources, Steps},
    ui::{Paint, PipelineResult, Step, Ui},
    workspace::ConfigWorkspace,
};
use activation::Activation;
use anyhow::bail;
use describe::Describe;
use omega_base::execution::Pipeline;
use omega_document::StateDocument;
use omega_host::{
    Layout, Profile,
    cargo::{BuildRequest, Selection},
};
use plan::Plan;
use std::time::Duration;
use system::System;

pub(crate) mod activation;
mod check;
mod describe;
mod plan;
pub(crate) mod steps;
mod system;
pub(crate) use check::Check;

pub(crate) struct Build {
    pub(crate) layout: Layout,
    pub(crate) profile: Profile,
    pub(crate) activation_timeout: Option<Duration>,
}

impl Build {
    /// Configuration builds share Omega's target directory and an explicit profile.
    pub(crate) fn request(layout: &Layout, profile: Profile, selection: Selection) -> BuildRequest {
        BuildRequest::new(selection)
            .profile(profile)
            .target_dir(layout.target_dir())
    }

    pub(crate) async fn run(&self, ui: &mut Ui) -> anyhow::Result<()> {
        let layout = &self.layout;
        let profile = self.profile;

        if !layout.workspace_manifest().exists() {
            bail!(
                "{} is not a Rust workspace — start one with {}",
                Paint::path(&layout.config),
                Paint::command("omega init")
            );
        }

        let workspace = ConfigWorkspace::open(layout.clone())?;
        let steps = Steps::production();
        let pipeline = Pipeline::new()
            .then(steps.compile)
            .then(steps.describe)
            .then(steps.validate)
            .then(steps.publish);
        let run = pipeline.run(Sources { workspace, profile }, ui).await;
        let published = PipelineResult::finish(run.result)?;
        let count = published.plugins;
        let generation = published.generation;

        ui.step(
            Step::Built,
            format!(
                "{} into {}",
                Paint::count(count, "plugin"),
                Paint::path(&layout.state)
            ),
        );
        ui.detail("Published for asynchronous daemon activation.");
        if let Some(timeout) = self.activation_timeout {
            ui.step(Step::Checking, "waiting for the daemon to apply this build");
            Activation { timeout }
                .wait_for(layout, &generation, &crate::operator::Operator::new())
                .await?;
            ui.step(
                Step::Checked,
                "build accepted and applied; use omega status for plugin health",
            );
        } else {
            ui.next("omega status");
        }
        Ok(())
    }

    /// Summarize the evaluated document.
    fn describe(document: &StateDocument) -> String {
        let counts = [
            (document.plugins.len(), "plugin"),
            (
                document.bars.len() + usize::from(!document.shell_json.is_empty()),
                "bar",
            ),
            (document.presentations.len(), "presentation"),
            (document.settings.len(), "setting"),
            (document.schedules.len(), "schedule"),
            (document.environment.len(), "variable"),
        ];

        let declared: Vec<String> = counts
            .iter()
            .filter(|(amount, _)| *amount > 0)
            .map(|(amount, noun)| Paint::count(*amount, noun))
            .collect();

        if declared.is_empty() {
            "an empty document".to_string()
        } else {
            declared.join(", ")
        }
    }
}
