use super::{CargoEditor, ConfigWorkspace, FileEdit, FileEdits, PluginName};
use crate::scaffold::Template;
use anyhow::{Context, Result, ensure};
use omega_host::workspace::cargo::CargoManifest;
use omega_host::{StageDir, Toml};

#[derive(Debug)]
pub(crate) struct PreparedPlugin<'a> {
    workspace: &'a ConfigWorkspace,
    name: PluginName,
    manifest: String,
    library: &'static str,
    main: Option<String>,
    destination: std::path::PathBuf,
    edits: FileEdits,
}

impl ConfigWorkspace {
    pub(crate) fn prepare_plugin(
        &self,
        name: PluginName,
        template: Template,
    ) -> Result<PreparedPlugin<'_>> {
        self.prepare_package(name, Some(template), &[std::path::PathBuf::from("system")])
    }

    pub(crate) fn prepare_library(
        &self,
        name: PluginName,
        consumers: &[std::path::PathBuf],
    ) -> Result<PreparedPlugin<'_>> {
        self.prepare_package(name, None, consumers)
    }

    fn prepare_package(
        &self,
        name: PluginName,
        template: Option<Template>,
        consumers: &[std::path::PathBuf],
    ) -> Result<PreparedPlugin<'_>> {
        use omega_host::workspace::WorkspaceRole;
        WorkspaceRole::check_layout(&self.layout)?;
        let destination = match template {
            Some(_) => self.layout.unit_src_dir(name.unit()),
            None => self.layout.library_src_dir(&name),
        };
        ensure!(
            !destination.try_exists()? && std::fs::symlink_metadata(&destination).is_err(),
            "{} already exists",
            destination.display()
        );
        let mut root = FileEdit::read(self.layout.workspace_manifest())?;
        ensure!(
            root.exists(),
            "this is not a config workspace; run omega init first"
        );
        let mut editor = CargoEditor::parse(root.source())?;
        editor.workspace(&self.scaffold.workspace_manifest())?;
        editor.member(
            destination
                .strip_prefix(&self.layout.config)?
                .to_str()
                .context("non-UTF-8 member path")?,
        )?;
        // Discovery needs only workspace fields, not a model of user package inheritance.
        let document: toml_edit::DocumentMut = root.source().parse()?;
        let mut workspace_only = toml_edit::DocumentMut::new();
        workspace_only["workspace"] = document["workspace"].clone();
        let manifest = Toml::decode::<CargoManifest>(&workspace_only.to_string())?;
        let members = manifest
            .workspace
            .context("missing workspace")?
            .member_dirs(&self.layout.config)?;
        for directory in &members {
            WorkspaceRole::at(&self.layout, directory)?;
            let path = directory.join("Cargo.toml");
            if path.try_exists()? {
                let source = std::fs::read_to_string(&path)?;
                let member = CargoEditor::parse(&source)?;
                let existing = member.package_name()?;
                ensure!(
                    existing.replace('-', "_") != name.rust_ident(),
                    "workspace package {existing} conflicts with {}",
                    name.package()
                );
            }
        }
        root.replace(editor.finish());
        let mut edits = vec![root];
        let mut seen = std::collections::BTreeSet::new();
        for consumer in consumers {
            ensure!(
                seen.insert(consumer),
                "duplicate consumer {}",
                consumer.display()
            );
            ensure!(
                consumer
                    .components()
                    .all(|c| matches!(c, std::path::Component::Normal(_))),
                "consumer must be a config member path"
            );
            let directory = self.layout.config.join(consumer);
            ensure!(
                members.contains(&directory),
                "consumer {} is not a workspace member",
                consumer.display()
            );
            let role = WorkspaceRole::at(&self.layout, &directory)?;
            let prefix = match role {
                WorkspaceRole::System => "..",
                WorkspaceRole::Plugin(_) | WorkspaceRole::Library(_) => "../..",
            };
            let relative = destination.strip_prefix(&self.layout.config)?;
            let mut edit = FileEdit::read(directory.join("Cargo.toml"))?;
            ensure!(
                edit.exists(),
                "consumer {} has no Cargo.toml",
                consumer.display()
            );
            let mut editor = CargoEditor::parse(edit.source())?;
            editor.package_name()?;
            editor.dependency(
                name.package(),
                &omega_host::workspace::cargo::Dependency::local(
                    format!("{prefix}/{}", relative.display()),
                    &[],
                ),
            )?;
            edit.replace(editor.finish());
            edits.push(edit);
        }
        let mut manifest = self.scaffold.unit_crate_manifest(name.unit());
        if template.is_none() {
            manifest.dependencies = Default::default();
        }
        let manifest = Toml::encode(&manifest)?;
        let main = template
            .map(|_| self.scaffold.unit_main(&name))
            .transpose()?;
        Ok(PreparedPlugin {
            workspace: self,
            name,
            manifest,
            library: template
                .map(Template::library)
                .unwrap_or("//! Shared types and components for this desktop.\n"),
            destination,
            main,
            edits: FileEdits::new(edits),
        })
    }
}

impl PreparedPlugin<'_> {
    pub(crate) fn apply(self) -> Result<PluginName> {
        let destination = self.destination;
        let stage =
            StageDir::in_directory(&destination, &self.workspace.layout.workspace_staging())?;
        stage.write("Cargo.toml", self.manifest.as_bytes())?;
        stage.write("src/lib.rs", self.library.as_bytes())?;
        if let Some(main) = self.main {
            stage.write("src/main.rs", main.as_bytes())?;
        }
        self.edits.apply()?;
        if let Err(error) = stage.publish_new() {
            return Err(self.edits.rollback(anyhow::anyhow!(
                "publishing {}: {error}; if the directory exists, inspect it before retrying",
                destination.display()
            )));
        }
        Ok(self.name)
    }
}
