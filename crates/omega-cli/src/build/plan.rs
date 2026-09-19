//! Describe staged binaries, then publish their manifests and document together.
use super::Describe;
use crate::ui::Paint;
use omega_document::{DocumentFile, StateDocument};
use omega_host::{BuiltPlugin, Generations, Layout, Profile, StateConfig, workspace::Plugins};
use omega_proto::{Manifest, PluginName};

/// Validated build inputs and resolved source paths for generation materialization.
pub(super) struct Plan {
    plugins: Vec<PluginBuild>,
    generation: omega_host::GenerationStage,
}

struct PluginBuild {
    name: PluginName,
    manifest: Manifest,
}

impl Plan {
    pub(super) fn manifests(&self) -> impl Iterator<Item = &Manifest> {
        self.plugins.iter().map(|plugin| &plugin.manifest)
    }

    /// Ask every built plugin what it declares.
    pub(super) async fn describe(
        plugins: &Plugins,
        layout: &Layout,
        profile: Profile,
    ) -> anyhow::Result<Self> {
        let generation = Generations::new(layout).stage()?;
        let staged = Layout::at(&layout.config, generation.files().path(), &layout.cache);

        let mut plan = Vec::with_capacity(plugins.len());
        for name in plugins {
            let program = layout.compiled_binary(profile, name);
            let manifest = Describe::program(&program, name).await?;
            let declared = manifest.plugin()?;
            if plan
                .iter()
                .any(|entry: &PluginBuild| entry.name == declared)
            {
                anyhow::bail!("duplicate executable identity {declared}");
            }
            generation
                .files()
                .copy(&program, layout.plugin_program_rel(&declared))?;
            let staged_manifest =
                Describe::program(&staged.state_plugin_program(&declared), &declared).await?;
            if manifest.canonical() != staged_manifest.canonical() {
                anyhow::bail!("{declared} changed while staging its executable");
            }
            plan.push(PluginBuild {
                manifest,
                name: declared,
            });
        }

        Ok(Self {
            plugins: plan,
            generation,
        })
    }

    pub(super) fn len(&self) -> usize {
        self.plugins.len()
    }

    /// Summarize aggregated manifest declarations.
    pub(super) fn grants(&self) -> String {
        let capabilities: std::collections::BTreeSet<&'static str> = self
            .plugins
            .iter()
            .flat_map(|plugin| plugin.manifest.granted().unwrap_or_default())
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

        let mut built = Vec::with_capacity(self.plugins.len());
        for plugin in &self.plugins {
            let entry = BuiltPlugin::new(layout, plugin.name.clone());

            // Preserve canonical bytes: the staged file and handshake hash must match.
            stage.write(&entry.manifest, &plugin.manifest.canonical())?;

            built.push(entry);
        }

        stage
            .file::<StateConfig>(StateConfig::FILE_NAME)
            .write(&StateConfig { plugins: built })?;

        // Publish binaries and their document as one generation.
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
