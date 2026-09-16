//! Run the `system/` binary and decode its desired-state document from stdout.
//! Validate the result before publication.

use anyhow::{Context, bail};

use omega_document::{DocumentFile, StateDocument};
use omega_host::{Layout, Profile};

#[derive(Debug)]
pub(super) struct System<'a> {
    layout: &'a Layout,
}

impl<'a> System<'a> {
    pub(super) fn new(layout: &'a Layout) -> Self {
        Self { layout }
    }

    /// Evaluate `system/`, or return an empty document if the crate is absent.
    /// Built plugins run by default even without a system document.
    pub(super) async fn evaluate(&self, profile: Profile) -> anyhow::Result<StateDocument> {
        if !self.layout.system_dir().exists() {
            return Ok(StateDocument::default());
        }

        let program = self.layout.compiled_system(profile);
        let output = tokio::process::Command::new(&program)
            .kill_on_drop(true)
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

        Ok(document)
    }
}
