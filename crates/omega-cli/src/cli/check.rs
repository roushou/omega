//! `omega check`: what each plugin declares, and whether the daemon can
//! grant it.
//!
//! This compiles. A plugin's manifest is the sum of its fields, so the only
//! thing that can say what a plugin declares is the plugin — and asking it
//! means building it first. What that buys is the thing it replaced: there is
//! no second file to check the code against, because there is no second file.

use anyhow::bail;

use omega_daemon::host::Units;
use omega_proto::{Layout, Profile};

use crate::cargo::Cargo;
use crate::describe::Describe;
use crate::ui::{Paint, Step, Ui};
use omega_renderer::{HostShell, Renderer};

/// Compile every plugin and report what it asks the daemon for.
#[derive(Debug, clap::Args)]
pub struct CheckCmd;

impl CheckCmd {
    /// Checking is not deploying: nobody waiting on an answer wants an
    /// optimiser.
    const PROFILE: Profile = Profile::Debug;

    pub async fn run(self, ui: &mut Ui) -> anyhow::Result<()> {
        let layout = Layout::resolve();
        if !layout.workspace_manifest().exists() {
            bail!(
                "{} is not a Rust workspace — start one with {}",
                Paint::path(&layout.config),
                Paint::command("omega init <name>")
            );
        }

        let units = Units::discover(&layout)?;
        if units.is_empty() {
            bail!("no plugins found in {}", Paint::path(layout.units_dir()));
        }

        ui.step(Step::Checking, Paint::count(units.len(), "plugin"));
        Cargo::new(&layout).build(Self::PROFILE).await?;

        // Report every plugin rather than stopping at the first: a check that
        // makes you re-run it once per mistake is worse than no check.
        let describe = Describe::new(&layout, Self::PROFILE);
        let mut failures = 0;

        for name in &units {
            match describe.manifest(name).await {
                Ok(manifest) => ui.item(
                    true,
                    format!(
                        "{}  {}",
                        Paint::name(name),
                        Paint::dim(Self::asks(&manifest))
                    ),
                ),
                Err(e) => {
                    failures += 1;
                    ui.item(
                        false,
                        format!("{}  {}", Paint::name(name), Paint::problem(e)),
                    );
                }
            }
        }

        Self::renderer(ui);

        match failures {
            0 => {
                ui.step(
                    Step::Checked,
                    format!("{}, all sound", Paint::count(units.len(), "plugin")),
                );
                Ok(())
            }
            n => bail!(
                "{n} of {} failed validation",
                Paint::count(units.len(), "plugin")
            ),
        }
    }

    /// Whether what draws these plugins is as old as they are.
    ///
    /// A renderer reads the wire format this binary writes, so one left
    /// behind by an older install is a widget that silently draws nothing —
    /// which is the hardest kind of wrong to find, because everything else
    /// reports success. Only a *stale* one is worth saying: a machine that
    /// puts nothing on its bar is entitled to have no renderer at all.
    fn renderer(ui: &mut Ui) {
        let Some(shell) = HostShell::detect() else {
            return;
        };
        let plugins = shell.plugins();

        for renderer in Renderer::ALL {
            if let Some(difference) = renderer.installed(&plugins).difference() {
                ui.warn(format!(
                    "{} draws these — {} — {}",
                    Paint::name(renderer.id),
                    difference,
                    Paint::command("omega shell install")
                ));
            }
        }
    }

    /// What a plugin asked for, in the words it asked in.
    ///
    /// The point of a derived manifest is that nobody typed it, so this is
    /// where a person finds out what their code added up to.
    fn asks(manifest: &omega_proto::Manifest) -> String {
        let mut asks = Vec::new();

        if !manifest.state_topics.is_empty() {
            asks.push(format!("reads {}", manifest.state_topics.join(", ")));
        }

        // Reading is already spelled out by the topics; what is left is what
        // the plugin can change.
        let effects: Vec<String> = manifest
            .capabilities
            .iter()
            .filter(|capability| *capability != "CAPABILITY_STATE_READ")
            .map(|capability| {
                capability
                    .strip_prefix("CAPABILITY_")
                    .unwrap_or(capability)
                    .to_lowercase()
            })
            .collect();
        if !effects.is_empty() {
            asks.push(format!("may {}", effects.join(", ")));
        }

        let surfaces: Vec<&str> = manifest
            .surfaces
            .iter()
            .map(|surface| surface.id.as_str())
            .collect();
        if !surfaces.is_empty() {
            asks.push(format!("serves {}", surfaces.join(", ")));
        }

        if asks.is_empty() {
            "declares nothing".to_string()
        } else {
            asks.join(" · ")
        }
    }
}
