//! `omega build`: compile the config workspace and assemble the state dir.

use anyhow::bail;

use omega_daemon::host::{Changes, Recursion, Units};
use omega_document::{DocumentFile, StateDocument};
use omega_host::{BuiltUnit, StateConfig};
use omega_host::{Generations, Layout, Profile};
use omega_proto::Manifest;
use omega_proto::UnitName;

use crate::cargo::Cargo;
use crate::describe::Describe;
use crate::system::System;
use crate::ui::{Paint, Step, Ui};

/// Compile `~/.config/omega` and assemble `~/.local/state/omega`.
#[derive(Debug, clap::Args)]
pub struct BuildCmd {
    /// Watch the config dir and rebuild on change.
    #[arg(long)]
    pub watch: bool,

    /// Compile without optimisations. Several times faster to produce, and
    /// what `omega dev` uses; the daemon runs whichever it is given.
    #[arg(long)]
    pub debug: bool,
}

impl BuildCmd {
    pub async fn run(self, ui: &mut Ui) -> anyhow::Result<()> {
        let layout = Layout::resolve();
        let profile = self.profile();
        if self.watch {
            return self.watch_loop(&layout, profile, ui).await;
        }
        Self::build_once(&layout, profile, ui).await
    }

    fn profile(&self) -> Profile {
        if self.debug {
            Profile::Debug
        } else {
            Profile::Release
        }
    }

    /// Rebuild whenever the config changes, which the kernel says rather
    /// than a timer guesses.
    async fn watch_loop(
        &self,
        layout: &Layout,
        profile: Profile,
        ui: &mut Ui,
    ) -> anyhow::Result<()> {
        let mut changes = Changes::watch(&[layout.config.as_path()], Recursion::Recursive)?;

        loop {
            if let Err(e) = Self::build_once(layout, profile, ui).await {
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

    async fn build_once(layout: &Layout, profile: Profile, ui: &mut Ui) -> anyhow::Result<()> {
        if !layout.workspace_manifest().exists() {
            bail!(
                "{} is not a Rust workspace — start one with {}",
                Paint::path(&layout.config),
                Paint::command("omega init")
            );
        }

        // 1. What there is to build. A plugin declares what it needs in its
        //    own code, so there is nothing to read until it is compiled.
        let units = Units::discover(layout)?;
        ui.step(
            Step::Building,
            format!(
                "{} in {}",
                Paint::count(units.len(), "plugin"),
                Paint::path(&layout.config)
            ),
        );

        // 2. Compile the workspace: the plugins, and the config plane with
        //    them. Cargo reports itself, on this same rail.
        Cargo::new(layout)
            .build(profile)
            .await
            .map_err(anyhow::Error::from)
            .map_err(|e| match crate::cli::link::LinkCmd::unlinked(layout) {
                Some(why) => e.context(why),
                None => e,
            })?;

        // 3. Ask each plugin what it declares. Its manifest is the sum of its
        //    fields, and only it can add them up.
        let plan = BuildPlan::describe(&units, layout, profile).await?;
        ui.step(
            Step::Declared,
            format!(
                "{} by {}",
                plan.grants(),
                Paint::count(plan.len(), "plugin")
            ),
        );

        // 4. Evaluate the configuration plane into a document. It runs in the
        //    user's own shell at build time, never in the daemon.
        let document = System::new(layout).evaluate(profile).await?;
        if layout.system_dir().exists() {
            ui.step(Step::Evaluated, Self::describe(&document));
        }

        // 5. Check what only the document can say. The daemon checks the
        //    same rule when it loads this, against the same grammar — but a
        //    schedule that will never fire should fail where somebody is
        //    still looking at the output, not in a log at three in the
        //    morning.
        omega_document::DocumentValidation::validate(
            &document,
            plan.units.iter().map(|unit| &unit.manifest),
        )?;

        // 6. Assemble the daemon's state atomically.
        let count = plan.len();
        plan.materialize(layout, &document)?;

        ui.step(
            Step::Built,
            format!(
                "{} into {}",
                Paint::count(count, "plugin"),
                Paint::path(&layout.state)
            ),
        );
        Ok(())
    }

    /// What the config plane said, in one line: a document is the point of
    /// the build, and "it ran" is not the same as knowing what it declared.
    fn describe(document: &StateDocument) -> String {
        let counts = [
            (document.units.len(), "unit"),
            (
                document.bars.len() + usize::from(!document.shell_json.is_empty()),
                "bar",
            ),
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

/// The build, decided up front: every manifest read and validated, every
/// source path resolved. Materializing it is a copy loop with no decisions
/// left in it.
struct BuildPlan {
    units: Vec<UnitBuild>,
    generation: omega_host::GenerationStage,
}

struct UnitBuild {
    name: UnitName,
    manifest: Manifest,
}

impl BuildPlan {
    /// Ask every built plugin what it declares.
    async fn describe(units: &Units, layout: &Layout, profile: Profile) -> anyhow::Result<Self> {
        let generation = Generations::new(layout).stage()?;
        let staged = Layout::at(&layout.config, generation.files().path(), &layout.cache);

        let mut plan = Vec::with_capacity(units.len());
        for name in units {
            generation.files().copy(
                &layout.compiled_binary(profile, name),
                layout.unit_program_rel(name),
            )?;
            plan.push(UnitBuild {
                manifest: Describe::program(&staged.state_unit_program(name), name).await?,
                name: name.clone(),
            });
        }

        Ok(Self {
            units: plan,
            generation,
        })
    }

    fn len(&self) -> usize {
        self.units.len()
    }

    /// What this build asked the daemon for, in one line: the point of
    /// deriving a manifest is that nobody typed it, so the build says what it
    /// derived.
    fn grants(&self) -> String {
        let capabilities: std::collections::BTreeSet<&'static str> = self
            .units
            .iter()
            .flat_map(|unit| unit.manifest.granted().unwrap_or_default())
            .map(|capability| capability.as_str_name())
            .collect();

        if capabilities.is_empty() {
            "no capabilities".to_string()
        } else {
            Paint::count(capabilities.len(), "capability")
        }
    }

    /// Complete and durably publish the generation containing the described binaries.
    fn materialize(self, layout: &Layout, document: &StateDocument) -> anyhow::Result<()> {
        let stage = self.generation.files();

        let mut built = Vec::with_capacity(self.units.len());
        for unit in &self.units {
            let entry = BuiltUnit::new(layout, unit.name.clone());

            // The canonical bytes, not a re-encoding of them: this file is
            // what the daemon hashes, so it must be what was hashed.
            stage.write(&entry.manifest, &unit.manifest.canonical())?;

            built.push(entry);
        }

        stage
            .file::<StateConfig>(StateConfig::FILE_NAME)
            .write(&StateConfig { units: built })?;

        // What was built, and what it is all for: the pair lands together or
        // not at all.
        DocumentFile::at(stage.path().join(DocumentFile::FILE_NAME)).write(document)?;

        if let Some(shell) = omega_document::shell::CompiledShell::of(document)? {
            let staged = Layout::at(&layout.config, stage.path(), &layout.cache);
            omega_host::AtomicFile::at(staged.compiled_shell())
                .write(shell.encode()?.as_bytes())?;
        }
        self.generation.commit()?;
        Ok(())
    }
}
