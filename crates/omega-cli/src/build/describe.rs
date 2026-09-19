//! Extract canonical manifest bytes from compiled plugins.
//! The staged manifest must match the bytes hashed during the plugin handshake.

use anyhow::{Context, bail};

use omega_host::{
    Layout, Profile,
    process::{OutputLimits, Process},
};
use omega_proto::{Manifest, PluginName};
use std::time::Duration;

#[derive(Debug)]
pub(super) struct Describe<'a> {
    layout: &'a Layout,
    profile: Profile,
}

impl<'a> Describe<'a> {
    pub(super) fn new(layout: &'a Layout, profile: Profile) -> Self {
        Self { layout, profile }
    }

    /// What one built plugin declares, validated against the plugin it is.
    pub(super) async fn manifest(&self, name: &PluginName) -> anyhow::Result<Manifest> {
        let program = self.layout.compiled_binary(self.profile, name);
        Self::program(&program, name).await
    }

    /// Describe the exact artifact that will be published.
    pub(super) async fn program(
        program: &std::path::Path,
        name: &PluginName,
    ) -> anyhow::Result<Manifest> {
        let mut command = tokio::process::Command::new(program);
        command.arg(Manifest::DESCRIBE);
        let output = Process::new(command)
            .timeout(Duration::from_secs(10))
            .capture(OutputLimits {
                stdout: 8 * 1024 * 1024,
                stderr: 64 * 1024,
            })
            .await
            .with_context(|| {
                format!(
                    "cannot run {} (is the crate name identical to the plugin name?)",
                    program.display()
                )
            })?;

        if !output.status.success() {
            bail!(
                "{name} could not describe itself ({}): {}",
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }

        let manifest = Manifest::decode_bytes(&output.stdout)
            .with_context(|| format!("{name} did not answer with a manifest"))?;

        manifest.validate(&manifest.plugin()?)?;
        Ok(manifest)
    }
}
