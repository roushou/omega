//! `omega build`: compile the config workspace and assemble the state dir.

use std::path::PathBuf;

use anyhow::{Context, bail};

use omega_daemon::host::{BuiltUnit, StateConfig};
use omega_daemon::host::{Changes, Recursion, StageDir, Units};
use omega_document::{DocumentFile, StateDocument};
use omega_host::{Layout, Profile};
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
                Paint::command("omega init <name>")
            );
        }

        // 1. What there is to build. A plugin declares what it needs in its
        //    own code, so there is nothing to read until it is compiled.
        let units = Units::discover(layout)?;
        if units.is_empty() {
            bail!("no plugins found in {}", Paint::path(layout.units_dir()));
        }
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
        let document = System::new(layout).evaluate(profile, &plan.names()).await?;
        if layout.system_dir().exists() {
            ui.step(Step::Evaluated, Self::describe(&document));
        }

        // 5. Check what only the document can say. The daemon checks the
        //    same rule when it loads this, against the same grammar — but a
        //    schedule that will never fire should fail where somebody is
        //    still looking at the output, not in a log at three in the
        //    morning.
        Self::check_cadences(&document)?;
        Self::check_keybinds(&document)?;

        // 6. Assemble the daemon's state atomically.
        plan.materialize(layout, &document)?;

        ui.step(
            Step::Built,
            format!(
                "{} into {}",
                Paint::count(plan.len(), "plugin"),
                Paint::path(&layout.state)
            ),
        );
        Ok(())
    }

    /// Every schedule's cadence, read with the grammar the daemon reads it
    /// with. Neither owns a private copy of the rule.
    fn check_cadences(document: &StateDocument) -> anyhow::Result<()> {
        for schedule in &document.schedules {
            schedule
                .parsed()
                .with_context(|| format!("schedule {:?} will never fire", schedule.id))?;
        }
        Ok(())
    }

    /// Keybinds are declarable and nothing converges them yet.
    ///
    /// Refused rather than staged, for the reason the daemon refuses an
    /// action it cannot perform: a bind that is accepted and never fires is
    /// worse than one that is turned away, because the first costs an
    /// afternoon to find. Delete this when a provider converges them.
    fn check_keybinds(document: &StateDocument) -> anyhow::Result<()> {
        let Some(keybind) = document.keybinds.first() else {
            return Ok(());
        };
        bail!(
            "keybind {:?}: the daemon does not converge keybinds yet, so this \
             would build and never fire",
            keybind.id
        )
    }

    /// What the config plane said, in one line: a document is the point of
    /// the build, and "it ran" is not the same as knowing what it declared.
    fn describe(document: &StateDocument) -> String {
        let counts = [
            (document.units.len(), "unit"),
            (document.bars.len(), "bar"),
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
}

struct UnitBuild {
    name: UnitName,
    manifest: Manifest,
    binary: PathBuf,
}

impl BuildPlan {
    /// Ask every built plugin what it declares.
    async fn describe(units: &Units, layout: &Layout, profile: Profile) -> anyhow::Result<Self> {
        let describe = Describe::new(layout, profile);

        let mut plan = Vec::with_capacity(units.len());
        for name in units {
            plan.push(UnitBuild {
                manifest: describe.manifest(name).await?,
                name: name.clone(),
                binary: layout.compiled_binary(profile, name),
            });
        }

        Ok(Self { units: plan })
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

    fn names(&self) -> Vec<UnitName> {
        self.units.iter().map(|unit| unit.name.clone()).collect()
    }

    /// Assemble the state dir in a staging directory, then swap it in.
    fn materialize(&self, layout: &Layout, document: &StateDocument) -> anyhow::Result<()> {
        let stage = StageDir::new(&layout.state)?;

        let mut built = Vec::with_capacity(self.units.len());
        for unit in &self.units {
            let entry = BuiltUnit::new(layout, unit.name.clone());

            // The canonical bytes, not a re-encoding of them: this file is
            // what the daemon hashes, so it must be what was hashed.
            stage.write(&entry.manifest, &unit.manifest.canonical())?;
            stage.copy(&unit.binary, &entry.program).with_context(|| {
                format!(
                    "unit {}: cannot copy compiled binary {} (is the crate name identical to the unit name?)",
                    unit.name,
                    unit.binary.display()
                )
            })?;

            built.push(entry);
        }

        stage
            .file::<StateConfig>(StateConfig::FILE_NAME)
            .write(&StateConfig { units: built })?;

        // What was built, and what it is all for: the pair lands together or
        // not at all.
        DocumentFile::at(stage.path().join(DocumentFile::FILE_NAME)).write(document)?;

        stage.commit()?;
        Ok(())
    }
}
