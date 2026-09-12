use super::{CargoEditor, ConfigWorkspace, FileEdit, FileEdits, PluginName};
use crate::scaffold::{Scaffold, Template};
use anyhow::{Context, Result, ensure};
use omega_daemon::host::cargo::CargoManifest;
use omega_host::{StageDir, Toml};

#[derive(Debug)]
pub(crate) struct PreparedPlugin<'a> {
    workspace: &'a ConfigWorkspace,
    name: PluginName,
    manifest: String,
    library: &'static str,
    main: String,
    edits: FileEdits,
}

impl ConfigWorkspace {
    pub(crate) fn prepare_plugin(
        &self,
        name: PluginName,
        template: Template,
    ) -> Result<PreparedPlugin<'_>> {
        let destination = self.layout.unit_src_dir(name.unit());
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
        let mut system = FileEdit::read(self.layout.system_manifest())?;
        ensure!(
            system.exists(),
            "missing system/Cargo.toml; run omega init first"
        );
        let mut editor = CargoEditor::parse(root.source())?;
        editor.workspace(&self.scaffold.workspace_manifest())?;
        editor.member(&format!("units/{}", name.package()))?;
        // Discovery needs only workspace fields, not a model of user package inheritance.
        let document: toml_edit::DocumentMut = root.source().parse()?;
        let mut workspace_only = toml_edit::DocumentMut::new();
        workspace_only["workspace"] = document["workspace"].clone();
        let manifest = Toml::decode::<CargoManifest>(&workspace_only.to_string())?;
        for directory in manifest
            .workspace
            .context("missing workspace")?
            .member_dirs(&self.layout.config)?
        {
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
        let mut editor = CargoEditor::parse(system.source())?;
        editor.package_name()?;
        editor.dependency(name.package(), &Scaffold::depends_on(name.unit()))?;
        system.replace(editor.finish());
        let manifest = Toml::encode(&self.scaffold.unit_crate_manifest(name.unit()))?;
        let main = self.scaffold.unit_main(&name)?;
        Ok(PreparedPlugin {
            workspace: self,
            name,
            manifest,
            library: template.library(),
            main,
            edits: FileEdits::new(vec![root, system]),
        })
    }
}

impl PreparedPlugin<'_> {
    pub(crate) fn apply(self) -> Result<PluginName> {
        let destination = self.workspace.layout.unit_src_dir(self.name.unit());
        let stage =
            StageDir::in_directory(&destination, &self.workspace.layout.workspace_staging())?;
        stage.write("Cargo.toml", self.manifest.as_bytes())?;
        stage.write("src/lib.rs", self.library.as_bytes())?;
        stage.write("src/main.rs", self.main.as_bytes())?;
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
