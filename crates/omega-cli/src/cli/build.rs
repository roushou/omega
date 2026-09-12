//! `omega build`: compile the config workspace and assemble the state dir.

use anyhow::{Context, bail};
use omega_proto::omega::{DeploymentStatus, ReconciliationState, ShellApplicationState};
use std::time::Duration;

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

    /// Wait for this build to be accepted, reconciled, and its shell applied.
    #[arg(long)]
    pub wait: bool,

    /// Maximum activation wait (seconds, optionally followed by s). Defaults to 30s.
    #[arg(long, requires = "wait", value_parser = Self::parse_timeout)]
    pub timeout: Option<Duration>,
}

impl BuildCmd {
    pub async fn run(self, ui: &mut Ui) -> anyhow::Result<()> {
        let layout = Layout::resolve();
        let profile = self.profile();
        if self.watch {
            return self.watch_loop(&layout, profile, ui).await;
        }
        self.build_once(&layout, profile, ui).await
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
        if self.wait {
            ui.step(Step::Checking, "waiting for the daemon to apply this build");
            self.wait_for(layout, &generation, &crate::operator::Operator::new())
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

    fn parse_timeout(input: &str) -> Result<Duration, String> {
        let seconds: u64 = input
            .strip_suffix('s')
            .unwrap_or(input)
            .parse()
            .map_err(|_| "expected a positive number of seconds, such as 30s".to_string())?;
        if seconds == 0 || seconds > 86400 {
            return Err("timeout must be between 1s and 86400s".into());
        }
        Ok(Duration::from_secs(seconds))
    }

    async fn wait_for(
        &self,
        layout: &Layout,
        generation: &omega_host::GenerationId,
        operator: &crate::operator::Operator,
    ) -> anyhow::Result<()> {
        let mut pending = "waiting for the daemon to accept the build".to_string();
        let timeout = self.timeout.unwrap_or(Duration::from_secs(30));
        tokio::time::timeout(timeout, async {
            loop {
                let published = Generations::new(layout).pin_current()?;
                if published.as_ref().map(|build| build.id()) != Some(generation) {
                    bail!("this build was superseded; use omega status to inspect the current build");
                }
                let status = operator.deployment().await
                    .context("cannot inspect build activation; ensure omega daemon is running")?;
                match Self::activation_pending(&status, generation.as_str())? {
                    None => return Ok(()),
                    Some(reason) => pending = reason,
                }
                tokio::time::sleep(Duration::from_millis(250)).await;
            }
        }).await.map_err(|_| anyhow::anyhow!(
            "build activation timed out after {}s: {pending}; use omega status. The build remains published and may activate later",
            timeout.as_secs()
        ))?
    }

    fn activation_pending(
        status: &DeploymentStatus,
        generation: &str,
    ) -> anyhow::Result<Option<String>> {
        if status.candidate_generation == generation && !status.activation_error.is_empty() {
            bail!("build activation failed: {}", status.activation_error);
        }
        if status.shell_generation == generation
            && status.shell == ShellApplicationState::Failed as i32
        {
            bail!(
                "shell application failed: {}; use omega shell diff",
                status.shell_error
            );
        }
        if status.accepted_generation != generation {
            return Ok(Some("waiting for the daemon to accept this build".into()));
        }
        match ReconciliationState::try_from(status.reconciliation) {
            Ok(ReconciliationState::Settled) => {}
            Ok(ReconciliationState::Pending | ReconciliationState::Unspecified) => {
                return Ok(Some(if status.reconciliation_error.is_empty() {
                    "waiting for configuration to be applied".into()
                } else {
                    status.reconciliation_error.clone()
                }));
            }
            Err(_) => bail!("unknown reconciliation state {}", status.reconciliation),
        }
        if status.shell_generation != generation {
            return Ok(Some("waiting for this build's shell result".into()));
        }
        match ShellApplicationState::try_from(status.shell) {
            Ok(ShellApplicationState::Applied | ShellApplicationState::NotDeclared) => Ok(None),
            Ok(ShellApplicationState::Applying | ShellApplicationState::Unspecified) => {
                Ok(Some("waiting for shell application".into()))
            }
            Ok(ShellApplicationState::Failed) => unreachable!("failure handled above"),
            Err(_) => bail!("unknown shell application state {}", status.shell),
        }
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
    fn materialize(
        self,
        layout: &Layout,
        document: &StateDocument,
    ) -> anyhow::Result<omega_host::GenerationId> {
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
        let id = self.generation.id();
        self.generation.commit()?;
        Ok(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn waiting_requires_this_build_and_its_shell_result() {
        let mut status = DeploymentStatus {
            accepted_generation: "old".into(),
            reconciliation: ReconciliationState::Settled as i32,
            shell_generation: "old".into(),
            shell: ShellApplicationState::Applied as i32,
            ..Default::default()
        };
        assert!(
            BuildCmd::activation_pending(&status, "new")
                .unwrap()
                .is_some()
        );
        status.accepted_generation = "new".into();
        assert!(
            BuildCmd::activation_pending(&status, "new")
                .unwrap()
                .is_some()
        );
        status.shell_generation = "new".into();
        assert!(
            BuildCmd::activation_pending(&status, "new")
                .unwrap()
                .is_none()
        );
        status.reconciliation = ReconciliationState::Pending as i32;
        status.reconciliation_error = "plugin not connected".into();
        assert_eq!(
            BuildCmd::activation_pending(&status, "new")
                .unwrap()
                .as_deref(),
            Some("plugin not connected")
        );
    }

    #[test]
    fn activation_and_shell_errors_only_fail_the_matching_build() {
        let mut status = DeploymentStatus {
            candidate_generation: "old".into(),
            activation_error: "invalid document".into(),
            shell_generation: "old".into(),
            shell: ShellApplicationState::Failed as i32,
            shell_error: "external edit".into(),
            ..Default::default()
        };
        assert!(
            BuildCmd::activation_pending(&status, "new")
                .unwrap()
                .is_some()
        );
        status.candidate_generation = "new".into();
        assert!(
            BuildCmd::activation_pending(&status, "new")
                .unwrap_err()
                .to_string()
                .contains("invalid document")
        );
        status.activation_error.clear();
        status.shell_generation = "new".into();
        assert!(
            BuildCmd::activation_pending(&status, "new")
                .unwrap_err()
                .to_string()
                .contains("omega shell diff")
        );
    }

    #[test]
    fn waiting_does_not_invent_a_process_health_gate() {
        let status = DeploymentStatus {
            accepted_generation: "build".into(),
            reconciliation: ReconciliationState::Settled as i32,
            shell_generation: "build".into(),
            shell: ShellApplicationState::NotDeclared as i32,
            units: vec![omega_proto::omega::UnitStatus {
                phase: omega_proto::omega::UnitPhase::Starting as i32,
                ..Default::default()
            }],
            ..Default::default()
        };
        assert!(
            BuildCmd::activation_pending(&status, "build")
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test(start_paused = true)]
    async fn waiting_has_a_deadline_and_never_unpublishes_or_follows_another_build() {
        let root = omega_host::TempPath::sibling(&std::env::temp_dir().join("omega-wait"), "test");
        let layout = Layout::at(root.join("config"), root.join("state"), root.join("cache"));
        let store = Generations::new(&layout);
        let stage = store.stage().unwrap();
        let id = stage.id();
        stage.commit().unwrap();
        let socket = omega_proto::Socket::at(root.join("control.sock"));
        let listener = socket.bind().unwrap();
        let operator = crate::operator::Operator::at(socket);
        let cmd = BuildCmd {
            watch: false,
            debug: true,
            wait: true,
            timeout: Some(Duration::from_secs(1)),
        };
        let started = tokio::time::Instant::now();
        let error = cmd.wait_for(&layout, &id, &operator).await.unwrap_err();
        assert!(error.to_string().contains("timed out"), "{error}");
        assert_eq!(started.elapsed(), Duration::from_secs(1));
        assert_eq!(store.pin_current().unwrap().unwrap().id(), &id);

        let other = store.stage().unwrap();
        let other_id = other.id();
        other.commit().unwrap();
        let error = cmd.wait_for(&layout, &id, &operator).await.unwrap_err();
        assert!(error.to_string().contains("superseded"), "{error}");
        assert_eq!(store.pin_current().unwrap().unwrap().id(), &other_id);
        drop(listener);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn timeout_arguments_are_explicit_and_bounded() {
        use clap::Parser;
        for args in [
            vec!["omega", "build", "--timeout", "3s"],
            vec!["omega", "build", "--wait", "--timeout", "0"],
            vec!["omega", "build", "--wait", "--timeout", "999999999999"],
            vec!["omega", "status", "--json", "--versions"],
        ] {
            assert!(crate::cli::Cli::try_parse_from(args).is_err());
        }
        assert_eq!(
            BuildCmd::parse_timeout("30s").unwrap(),
            Duration::from_secs(30)
        );
    }
}
