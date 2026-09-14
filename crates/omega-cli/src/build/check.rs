//! Compile manifests and evaluate the desired-state document without publishing.
//! Manifest validation requires running the built plugins; `cargo check` alone
//! cannot determine what their fields declare.

use anyhow::bail;

use omega_host::workspace::Plugins;
use omega_host::{Layout, Profile};
use omega_proto::omega::Capability;

use crate::build::cargo::Cargo;
use crate::build::describe::Describe;
use crate::build::system::System;
use crate::ui::{Paint, Step, Ui};
use omega_omarchy::HostShell;
use omega_omarchy::Renderer;

/// Compile and validate the configuration and plugins without publishing a generation.
#[derive(Debug)]
pub(crate) struct Check {
    pub(crate) layout: Layout,
}

impl Check {
    /// Use the debug profile for validation builds.
    const PROFILE: Profile = Profile::Debug;

    pub(crate) async fn run(self, ui: &mut Ui) -> anyhow::Result<()> {
        let layout = self.layout;
        if !layout.workspace_manifest().exists() {
            bail!(
                "{} is not a Rust workspace — start one with {}",
                Paint::path(&layout.config),
                Paint::command("omega init")
            );
        }

        let _workspace = crate::workspace::ConfigWorkspace::open(layout.clone())?;
        let units = Plugins::discover(&layout)?;

        ui.step(Step::Checking, Paint::count(units.len(), "plugin"));
        Cargo::new(&layout).build(Self::PROFILE).await?;

        // Collect validation results for every plugin.
        let describe = Describe::new(&layout, Self::PROFILE);
        let mut failures = 0;
        let mut manifests = Vec::with_capacity(units.len());

        for name in &units {
            match describe.manifest(name).await {
                Ok(manifest) => {
                    ui.item(
                        true,
                        format!(
                            "{}  {}",
                            Paint::name(name),
                            Paint::dim(Self::asks(&manifest))
                        ),
                    );
                    manifests.push(manifest);
                }
                Err(e) => {
                    failures += 1;
                    ui.item(
                        false,
                        format!("{}  {}", Paint::name(name), Paint::problem(e)),
                    );
                }
            }
        }

        if failures > 0 {
            bail!(
                "{failures} of {} failed validation",
                Paint::count(units.len(), "plugin")
            );
        }
        let document = System::new(&layout).evaluate(Self::PROFILE).await?;
        omega_omarchy::DocumentValidation::validate(&document, &manifests)?;
        Self::renderer(ui);
        ui.step(
            Step::Checked,
            format!(
                "configuration and {} validated; no generation published",
                Paint::count(units.len(), "plugin"),
            ),
        );
        Ok(())
    }

    /// Warn if an installed renderer differs from the embedded version.
    /// A missing renderer is permitted for configurations without UI.
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

    /// Report the capabilities and subscriptions declared by a plugin.
    fn asks(manifest: &omega_proto::Manifest) -> String {
        let mut asks = Vec::new();

        if !manifest.state_topics.is_empty() {
            asks.push(format!("reads {}", manifest.state_topics.join(", ")));
        }

        // Read capabilities are already represented by the topic list.
        let effects: Vec<String> = manifest
            .granted()
            .unwrap_or_default()
            .into_iter()
            .filter(|capability| *capability != Capability::StateRead)
            .map(|capability| {
                capability
                    .as_str_name()
                    .strip_prefix("CAPABILITY_")
                    .unwrap_or_default()
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
