//! Compile, validate, and publish config generations without owning CLI syntax.
use crate::ui::{Paint, Step, Ui};
use activation::Activation;
use anyhow::bail;
use cargo::Cargo;
use describe::Describe;
use omega_document::StateDocument;
use omega_host::{
    Layout, Profile,
    fs::{Changes, Recursion},
    workspace::Plugins,
};
use plan::Plan;
use std::time::Duration;
use system::System;

mod activation;
pub(crate) mod cargo;
mod check;
mod describe;
mod plan;
mod system;
pub(crate) use check::Check;

pub(crate) struct Build {
    pub(crate) layout: Layout,
    pub(crate) profile: Profile,
    pub(crate) watch: bool,
    pub(crate) activation_timeout: Option<Duration>,
}
impl Build {
    pub(crate) async fn run(&self, ui: &mut Ui) -> anyhow::Result<()> {
        if self.watch {
            self.watch_loop(&self.layout, self.profile, ui).await
        } else {
            self.build_once(&self.layout, self.profile, ui).await
        }
    }
    /// Rebuild on settled filesystem changes.
    async fn watch_loop(
        &self,
        layout: &Layout,
        profile: Profile,
        ui: &mut Ui,
    ) -> anyhow::Result<()> {
        let mut changes = Changes::watch(&[layout.config.as_path()], Recursion::Recursive)?;

        loop {
            if let Err(e) = self.build_once(layout, profile, ui).await {
                // A watch outlives a failed build: the next save is the fix.
                ui.error(&e);
            }
            ui.step(Step::Watching, Paint::path(&layout.config));

            // Events raised while the build was running are already waiting,
            // so an edit during a build is not missed.
            if changes.next().await.is_none() {
                return Ok(());
            }
            ui.blank();
            ui.step(Step::Changed, "rebuilding");
        }
    }

    async fn build_once(
        &self,
        layout: &Layout,
        profile: Profile,
        ui: &mut Ui,
    ) -> anyhow::Result<()> {
        if !layout.workspace_manifest().exists() {
            bail!(
                "{} is not a Rust workspace — start one with {}",
                Paint::path(&layout.config),
                Paint::command("omega init")
            );
        }

        let _workspace = crate::workspace::ConfigWorkspace::open(layout.clone())?;

        let units = Plugins::discover(layout)?;
        ui.step(
            Step::Building,
            format!(
                "{} in {}",
                Paint::count(units.len(), "plugin"),
                Paint::path(&layout.config)
            ),
        );

        Cargo::new(layout)
            .build(profile)
            .await
            .map_err(anyhow::Error::from)
            .map_err(|e| match crate::checkout::CheckoutLink::unlinked(layout) {
                Some(why) => e.context(why),
                None => e,
            })?;

        let plan = Plan::describe(&units, layout, profile).await?;
        ui.step(
            Step::Declared,
            format!(
                "{} by {}",
                plan.grants(),
                Paint::count(plan.len(), "plugin")
            ),
        );

        let document = System::new(layout).evaluate(profile).await?;
        if layout.system_dir().exists() {
            ui.step(Step::Evaluated, Self::describe(&document));
        }

        omega_omarchy::DocumentValidation::validate(&document, plan.manifests())?;

        let count = plan.len();
        let generation = plan.materialize(layout, &document)?;

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
            (document.units.len(), "unit"),
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
