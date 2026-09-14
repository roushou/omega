//! Extract canonical manifest bytes from compiled plugins.
//! The staged manifest must match the bytes hashed during the plugin handshake.

use anyhow::{Context, bail};

use omega_host::{Layout, Profile};
use omega_proto::{Manifest, UnitName};

#[derive(Debug)]
pub(super) struct Describe<'a> {
    layout: &'a Layout,
    profile: Profile,
}

impl<'a> Describe<'a> {
    pub(super) fn new(layout: &'a Layout, profile: Profile) -> Self {
        Self { layout, profile }
    }

    /// What one built plugin declares, validated against the unit it is.
    pub(super) async fn manifest(&self, name: &UnitName) -> anyhow::Result<Manifest> {
        let program = self.layout.compiled_binary(self.profile, name);
        Self::program(&program, name).await
    }

    /// Describe the exact artifact that will be published.
    pub(super) async fn program(
        program: &std::path::Path,
        name: &UnitName,
    ) -> anyhow::Result<Manifest> {
        let output = tokio::process::Command::new(program)
            .arg(Manifest::DESCRIBE)
            .kill_on_drop(true)
            .output()
            .await
            .with_context(|| {
                format!(
                    "cannot run {} (is the crate name identical to the unit name?)",
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

        // The plugin names itself from its crate; a mismatch means the binary
        // in this crate's output is not this crate's.
        manifest.validate(name)?;
        Ok(manifest)
    }
}
