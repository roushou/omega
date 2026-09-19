use super::{ConfigWorkspace, FileEdit, FileEdits};
use crate::scaffold::Template;
use anyhow::{Context, Result, ensure};
use omega_host::cargo::{CargoSlot, Dependency, Manifest};
use omega_host::package::PackageName;
use omega_host::{StageDir, Toml};

#[derive(Debug)]
pub(crate) struct PreparedPackage<'a> {
    workspace: &'a ConfigWorkspace,
    name: PackageName,
    manifest: String,
    files: Vec<(&'static str, String)>,
    destination: std::path::PathBuf,
    edits: FileEdits,
}

#[derive(Debug, Clone, Copy)]
enum PackageKind {
    Plugin(Template),
    Library,
    CommandHost,
}

impl ConfigWorkspace {
    pub(crate) fn prepare_plugin(
        &self,
        name: PackageName,
        template: Template,
    ) -> Result<PreparedPackage<'_>> {
        self.prepare_package(
            name,
            PackageKind::Plugin(template),
            &[std::path::PathBuf::from("system")],
        )
    }

    pub(crate) fn prepare_library(
        &self,
        name: PackageName,
        consumers: &[std::path::PathBuf],
    ) -> Result<PreparedPackage<'_>> {
        self.prepare_package(name, PackageKind::Library, consumers)
    }

    pub(crate) fn prepare_command_host(&self, name: PackageName) -> Result<PreparedPackage<'_>> {
        self.prepare_package(
            name,
            PackageKind::CommandHost,
            &[std::path::PathBuf::from("system")],
        )
    }

    fn prepare_package(
        &self,
        name: PackageName,
        kind: PackageKind,
        consumers: &[std::path::PathBuf],
    ) -> Result<PreparedPackage<'_>> {
        use omega_host::workspace::WorkspaceRole;
        let (destination, pattern) = match kind {
            PackageKind::Plugin(_) => (self.layout.plugin_src_dir(name.plugin()), "plugins/*"),
            PackageKind::Library => (self.layout.library_src_dir(&name), "crates/*"),
            PackageKind::CommandHost => (self.layout.command_src_dir(&name), "commands/*"),
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
        let mut editor = root.source().parse::<Manifest>()?;
        self.scaffold.complete_workspace(&mut editor)?;
        editor.ensure_member(
            destination
                .strip_prefix(&self.layout.config)?
                .to_str()
                .context("non-UTF-8 member path")?,
            // Cargo rejects unmatched globs, so add each one with its first crate.
            pattern,
        )?;
        let updated = editor.to_string();
        let members = editor
            .workspace()?
            .context("missing workspace")?
            .member_dirs(&self.layout.config)?;
        for directory in &members {
            WorkspaceRole::at(&self.layout, directory)?;
            let path = self
                .layout
                .file::<Manifest>(CargoSlot::Member(directory))
                .into_path();
            if path.try_exists()? {
                let source = std::fs::read_to_string(&path)?;
                let member = source.parse::<Manifest>()?;
                let existing = member.package()?.context("missing package")?.name()?;
                ensure!(
                    existing.replace('-', "_") != name.rust_ident(),
                    "workspace package {existing} conflicts with {}",
                    name.package()
                );
            }
        }
        root.replace(updated);
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
                WorkspaceRole::Plugin(_)
                | WorkspaceRole::Library(_)
                | WorkspaceRole::Commands(_) => "../..",
            };
            let relative = destination.strip_prefix(&self.layout.config)?;
            let mut edit = FileEdit::read(
                self.layout
                    .file::<Manifest>(CargoSlot::Member(&directory))
                    .into_path(),
            )?;
            ensure!(
                edit.exists(),
                "consumer {} has no Cargo.toml",
                consumer.display()
            );
            let mut editor = edit.source().parse::<Manifest>()?;
            editor.package()?.context("missing package")?.name()?;
            editor.ensure_path_dependency(
                name.package(),
                &Dependency::local(format!("{prefix}/{}", relative.display()), &[]),
            )?;
            edit.replace(editor.to_string());
            edits.push(edit);
        }
        let (manifest, files) = match kind {
            PackageKind::Plugin(template) => (
                self.scaffold.plugin_crate_manifest(name.plugin())?,
                vec![
                    ("src/lib.rs", template.library().into()),
                    ("src/main.rs", self.scaffold.plugin_main(&name)?),
                ],
            ),
            PackageKind::Library => {
                let mut manifest = self.scaffold.plugin_crate_manifest(name.plugin())?;
                manifest.clear_dependencies();
                (
                    manifest,
                    vec![(
                        "src/lib.rs",
                        "//! Shared types and components for this desktop.\n".into(),
                    )],
                )
            }
            PackageKind::CommandHost => (
                self.scaffold.command_host_manifest(&name)?,
                self.scaffold.command_host_files(&name)?,
            ),
        };
        Ok(PreparedPackage {
            workspace: self,
            name,
            manifest: Toml::encode(&manifest)?,
            files,
            destination,
            edits: FileEdits::new(edits),
        })
    }
}

impl PreparedPackage<'_> {
    pub(crate) fn apply(self) -> Result<PackageName> {
        let destination = self.destination;
        let stage =
            StageDir::in_directory(&destination, &self.workspace.layout.workspace_staging())?;
        stage.write("Cargo.toml", self.manifest.as_bytes())?;
        for (path, source) in self.files {
            stage.write(path, source.as_bytes())?;
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
