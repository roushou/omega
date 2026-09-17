//! Named build operations and the typed artifacts passed between them.

use super::{Build, Plan, System};
use crate::workspace::ConfigWorkspace;
use omega_base::execution::{Operation, Progress, Step};
use omega_document::StateDocument;
use omega_host::cargo::{Cargo, Selection};
use omega_host::{GenerationId, Layout, Profile, workspace::Plugins};

pub(crate) struct Sources {
    pub(crate) workspace: ConfigWorkspace,
    pub(crate) profile: Profile,
}

pub(crate) struct Compiled {
    sources: Sources,
    units: Plugins,
}

pub(crate) struct Described {
    sources: Sources,
    plan: Plan,
}

pub(crate) struct Validated {
    sources: Sources,
    plan: Plan,
    document: StateDocument,
}

pub(crate) struct Published {
    pub(crate) layout: Layout,
    pub(crate) generation: GenerationId,
    pub(crate) plugins: usize,
}

pub(crate) struct Compile;
pub(crate) struct DescribePlugins;
pub(crate) struct Validate;
pub(crate) struct Publish;

impl Operation<Sources> for Compile {
    type Output = Compiled;
    type Error = anyhow::Error;

    async fn execute(
        self,
        sources: Sources,
        progress: &mut Progress<'_>,
    ) -> anyhow::Result<Compiled> {
        let layout = sources.workspace.layout();
        progress.path("Workspace:", &layout.config);

        let units = Plugins::discover(layout)?;
        Cargo::new(&layout.config)
            .build(Build::request(
                layout,
                sources.profile,
                Selection::Workspace,
            ))
            .await
            .map_err(anyhow::Error::from)
            .map_err(
                |error| match crate::checkout::CheckoutLink::unlinked(layout) {
                    Some(why) => error.context(why),
                    None => error,
                },
            )?;

        Ok(Compiled { sources, units })
    }
}

impl Operation<Compiled> for DescribePlugins {
    type Output = Described;
    type Error = anyhow::Error;

    async fn execute(
        self,
        compiled: Compiled,
        progress: &mut Progress<'_>,
    ) -> anyhow::Result<Described> {
        let plan = Plan::describe(
            &compiled.units,
            compiled.sources.workspace.layout(),
            compiled.sources.profile,
        )
        .await?;
        progress.message(plan.grants());

        Ok(Described {
            sources: compiled.sources,
            plan,
        })
    }
}

impl Operation<Described> for Validate {
    type Output = Validated;
    type Error = anyhow::Error;

    async fn execute(
        self,
        described: Described,
        progress: &mut Progress<'_>,
    ) -> anyhow::Result<Validated> {
        let document = System::new(described.sources.workspace.layout())
            .evaluate(described.sources.profile)
            .await?;
        progress.message(super::Build::describe(&document));
        described.validate(document)
    }
}

impl Operation<Validated> for Publish {
    type Output = Published;
    type Error = anyhow::Error;

    async fn execute(
        self,
        validated: Validated,
        progress: &mut Progress<'_>,
    ) -> anyhow::Result<Published> {
        let layout = validated.sources.workspace.layout().clone();
        let plugins = validated.plan.len();
        let generation = validated.plan.materialize(&layout, &validated.document)?;

        // Publication ends the source lock before asynchronous activation waits.
        drop(validated.sources);
        progress.message(format!("Published generation: {generation}"));

        Ok(Published {
            layout,
            generation,
            plugins,
        })
    }
}

pub(crate) struct Steps {
    pub(crate) compile: Step<Sources, Compiled, anyhow::Error>,
    pub(crate) describe: Step<Compiled, Described, anyhow::Error>,
    pub(crate) validate: Step<Described, Validated, anyhow::Error>,
    pub(crate) publish: Step<Validated, Published, anyhow::Error>,
}

impl Steps {
    pub(crate) fn isolated() -> Self {
        Self {
            compile: Step::new("build.compile", "compile configuration"),
            describe: Step::new("build.describe", "validate plugin manifests"),
            validate: Step::new(
                "build.evaluate",
                "evaluate and validate the system document",
            ),
            publish: Step::new("build.publish", "publish the validated generation"),
        }
    }

    pub(crate) fn production() -> Self {
        let mut steps = Self::isolated();

        steps.compile.replace(Compile);
        steps.describe.replace(DescribePlugins);
        steps.validate.replace(Validate);
        steps.publish.replace(Publish);

        steps
    }
}

impl Validated {
    pub(crate) fn layout(&self) -> &Layout {
        self.sources.workspace.layout()
    }
}

impl Described {
    pub(crate) fn validate(self, document: StateDocument) -> anyhow::Result<Validated> {
        omega_omarchy::DocumentValidation::validate(&document, self.plan.manifests())?;

        Ok(Validated {
            sources: self.sources,
            plan: self.plan,
            document,
        })
    }
}

#[cfg(test)]
impl Compiled {
    pub(crate) fn fixture(sources: Sources) -> anyhow::Result<Self> {
        let units = Plugins::discover(sources.workspace.layout())?;
        anyhow::ensure!(units.is_empty(), "fixture does not compile plugin binaries");

        Ok(Self { sources, units })
    }
}
