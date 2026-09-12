//! Validated, pinned inputs for one build adoption.
use crate::DaemonError;
use crate::manifest::ManifestStore;
use omega_document::{DocumentFile, StateDocument};
use omega_host::{Generation, Generations, Layout, StateConfig};
use std::collections::BTreeSet;
use std::io::Read;
use std::sync::Arc;

/// One validated, leased input set shared by activation and its providers.
#[derive(Debug)]
pub struct ValidatedBuild {
    pub(super) generation: Generation,
    pub(super) config: StateConfig,
    pub(super) manifests: Arc<ManifestStore>,
    pub(super) document: StateDocument,
}

impl ValidatedBuild {
    pub(super) fn load(
        root: &Layout,
        deployment: &super::deployment::Deployment,
    ) -> Result<Option<Self>, DaemonError> {
        let Some(generation) = Generations::new(root).pin_current()? else {
            return Ok(None);
        };
        deployment.candidate(generation.id());
        Self::read(generation)
            .inspect_err(|error| deployment.activation_failed(error))
            .map(Some)
    }

    /// Validate a leased generation without changing references or live state.
    pub fn read(generation: Generation) -> Result<Self, DaemonError> {
        let layout = generation.layout();
        let config = layout.file::<StateConfig>(()).read()?;
        let manifests = Arc::new(ManifestStore::load(&config, layout)?);
        let document = DocumentFile::of(layout).read()?;
        omega_document::DocumentValidation::validate(
            &document,
            manifests.iter().map(|(_, entry)| &entry.manifest),
        )
        .map_err(|error| crate::reconcile::ProviderError::new("document", error.to_string()))?;
        if let Some(shell) =
            omega_document::shell::CompiledShell::of(&document).map_err(std::io::Error::other)?
        {
            let staged: serde_json::Value =
                serde_json::from_slice(&std::fs::read(layout.compiled_shell())?)
                    .map_err(std::io::Error::other)?;
            if &staged != shell.config() {
                return Err(std::io::Error::other(
                    "staged shell configuration differs from its document",
                )
                .into());
            }
        }
        let mut names = BTreeSet::new();
        for unit in &config.units {
            if !names.insert(&unit.name)
                || unit.program != layout.unit_program_rel(&unit.name)
                || unit.manifest != layout.unit_manifest_rel(&unit.name)
            {
                return Err(
                    std::io::Error::other("invalid or duplicate build descriptor entry").into(),
                );
            }
            let path = layout.state_unit_program(&unit.name);
            let metadata = std::fs::symlink_metadata(path)?;
            use std::os::unix::fs::PermissionsExt;
            if !metadata.is_file() || metadata.permissions().mode() & 0o111 == 0 {
                return Err(std::io::Error::other(format!(
                    "{} is not an executable file",
                    unit.name
                ))
                .into());
            }
        }
        Ok(Self {
            generation,
            config,
            manifests,
            document,
        })
    }

    /// Apply the shell belonging to this validated generation.
    pub fn apply_shell(
        &self,
        root: &Layout,
        overwrite: bool,
        deployment: &super::deployment::Deployment,
    ) -> Result<(), super::shell::ShellApplyError> {
        deployment.applying_shell(self.generation.id(), !self.document.shell_json.is_empty());
        if self.document.shell_json.is_empty() {
            return Ok(());
        }
        let result = (|| {
            let shell = omega_document::shell::CompiledShell::of(&self.document)?
                .expect("nonempty shell declaration compiles to a shell");
            omega_host::shell::ShellInstallation::new(root).apply(
                shell.config(),
                self.generation.id(),
                overwrite,
            )?;
            Ok(())
        })();
        deployment.shell_applied(result.as_ref().err());
        result
    }

    pub(super) fn changed_units(
        &self,
        next: &Self,
    ) -> Result<Vec<omega_proto::UnitName>, DaemonError> {
        let mut changed = Vec::new();
        for name in self.config.names() {
            let same_manifest = self.manifests.get(name).map(|entry| &entry.hash)
                == next.manifests.get(name).map(|entry| &entry.hash);
            if !same_manifest
                || self.settings(name) != next.settings(name)
                || !Self::same_program(
                    &self.generation.layout().state_unit_program(name),
                    &next.generation.layout().state_unit_program(name),
                )?
            {
                changed.push(name.clone());
            }
        }
        Ok(changed)
    }

    pub(super) fn settings(
        &self,
        name: &omega_proto::UnitName,
    ) -> std::collections::HashMap<String, omega_proto::omega::Value> {
        self.document
            .units
            .iter()
            .find(|unit| unit.name == name.as_str())
            .map(|unit| unit.config.clone())
            .unwrap_or_default()
    }

    fn same_program(left: &std::path::Path, right: &std::path::Path) -> std::io::Result<bool> {
        let mut left = std::fs::File::open(left)?;
        let mut right = std::fs::File::open(right)?;
        let mut remaining = left.metadata()?.len();
        if remaining != right.metadata()?.len() {
            return Ok(false);
        }
        let mut a = [0; 65536];
        let mut b = [0; 65536];
        while remaining > 0 {
            let count = remaining.min(a.len() as u64) as usize;
            left.read_exact(&mut a[..count])?;
            right.read_exact(&mut b[..count])?;
            if a[..count] != b[..count] {
                return Ok(false);
            }
            remaining -= count as u64;
        }
        Ok(true)
    }
}
