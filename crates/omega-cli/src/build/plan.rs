//! Describe staged binaries, then publish their manifests and document together.
use super::Describe;
use crate::ui::Paint;
use omega_document::{DocumentFile, StateDocument};
use omega_host::{BuiltUnit, Generations, Layout, Profile, StateConfig, workspace::Plugins};
use omega_proto::{Manifest, UnitName};

/// The build, decided up front: every manifest read and validated, every
/// source path resolved. Materializing it is a copy loop with no decisions
/// left in it.
pub(super) struct Plan {
    units: Vec<UnitBuild>,
    generation: omega_host::GenerationStage,
}

struct UnitBuild {
    name: UnitName,
    manifest: Manifest,
}

impl Plan {
    pub(super) fn manifests(&self) -> impl Iterator<Item = &Manifest> {
        self.units.iter().map(|unit| &unit.manifest)
    }

    /// Ask every built plugin what it declares.
    pub(super) async fn describe(
        units: &Plugins,
        layout: &Layout,
        profile: Profile,
    ) -> anyhow::Result<Self> {
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

    pub(super) fn len(&self) -> usize {
        self.units.len()
    }

    /// What this build asked the daemon for, in one line: the point of
    /// deriving a manifest is that nobody typed it, so the build says what it
    /// derived.
    pub(super) fn grants(&self) -> String {
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
    pub(super) fn materialize(
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

        if let Some(shell) = omega_omarchy::shell::CompiledShell::of(document)? {
            let staged = Layout::at(&layout.config, stage.path(), &layout.cache);
            omega_host::AtomicFile::at(staged.compiled_shell())
                .write(shell.encode()?.as_bytes())?;
        }
        let id = self.generation.id();
        self.generation.commit()?;
        Ok(id)
    }
}
