//! Evaluating the configuration plane.
//!
//! `system/` is a crate that computes a state document and prints it. Running
//! it is the only way to learn what a config means, because a config is a
//! program — but it is a program with one entry point and no side effects, so
//! the build runs it, reads its answer, and validates it before anything else
//! sees it.

use anyhow::{Context, bail};

use omega_document::{DocumentFile, StateDocument};
use omega_proto::{Layout, Profile, UnitName};

#[derive(Debug)]
pub struct System<'a> {
    layout: &'a Layout,
}

impl<'a> System<'a> {
    pub fn new(layout: &'a Layout) -> Self {
        Self { layout }
    }

    /// The document this config declares, or an empty one when the config has
    /// no `system/` crate — a workspace of units and nothing else is a valid
    /// config, and every unit in it runs.
    pub async fn evaluate(
        &self,
        profile: Profile,
        built: &[UnitName],
    ) -> anyhow::Result<StateDocument> {
        if !self.layout.system_dir().exists() {
            return Ok(StateDocument::default());
        }

        let program = self.layout.compiled_system(profile);
        let output = tokio::process::Command::new(&program)
            .output()
            .await
            .with_context(|| format!("cannot run {}", program.display()))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!(
                "the config plane failed ({}): {}",
                output.status,
                stderr.trim()
            );
        }

        let document = DocumentFile::parse(&String::from_utf8_lossy(&output.stdout))
            .context("the config plane did not emit a state document")?;

        Self::validate(&document, built)?;
        Ok(document)
    }

    /// A document that names a unit this build does not contain is a typo
    /// with consequences: it would silently do nothing at runtime.
    fn validate(document: &StateDocument, built: &[UnitName]) -> anyhow::Result<()> {
        let unknown: Vec<&str> = document
            .units
            .iter()
            .map(|unit| unit.name.as_str())
            .filter(|name| !built.iter().any(|built| built.as_str() == *name))
            .collect();

        if unknown.is_empty() {
            Ok(())
        } else {
            bail!(
                "the config plane names unit(s) this workspace does not build: {}",
                unknown.join(", ")
            )
        }
    }
}
