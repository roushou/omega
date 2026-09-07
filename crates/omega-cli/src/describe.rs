//! Asking a compiled plugin what it declares.
//!
//! A plugin's manifest is not a file anybody writes. It is what the plugin's
//! own fields add up to — a `Battery` field is a topic and the permission to
//! read it — and the only thing that can add them up is the plugin. So the
//! build compiles it and asks, exactly as it asks the config plane what the
//! machine should be.
//!
//! The manifest and the binary therefore cannot disagree: the one on disk is
//! the one this binary answered with, and a swapped binary answers with a
//! different hash and is refused at the handshake.

use anyhow::{Context, bail};

use omega_proto::Manifest;
use omega_proto::Validated;
use omega_proto::{Layout, Profile, UnitName};

#[derive(Debug)]
pub struct Describe<'a> {
    layout: &'a Layout,
    profile: Profile,
}

impl<'a> Describe<'a> {
    pub fn new(layout: &'a Layout, profile: Profile) -> Self {
        Self { layout, profile }
    }

    /// What one built plugin declares, validated against the unit it is.
    pub async fn manifest(&self, name: &UnitName) -> anyhow::Result<Manifest> {
        let program = self.layout.compiled_binary(self.profile, name);

        let output = tokio::process::Command::new(&program)
            .arg(Manifest::DESCRIBE)
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

        let manifest = Manifest::parse(&String::from_utf8_lossy(&output.stdout))
            .with_context(|| format!("{name} did not answer with a manifest"))?;

        // The plugin names itself from its crate; a mismatch means the binary
        // in this crate's output is not this crate's.
        manifest.validate(name)?;
        Ok(manifest)
    }
}
