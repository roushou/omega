use super::{ConfigWorkspace, FileEdit, FileEdits};
use anyhow::{Context, Result, ensure};
use omega_host::{AtomicFile, Directory, Layout};
use std::path::{Component, Path, PathBuf};
use toml_edit::{DocumentMut, Item, Value};

/// The journal is durable before the first edit and removed after publication.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct Migration {
    edits: FileEdits,
}

impl ConfigWorkspace {
    pub(crate) fn prepare_migration(&self) -> Result<Option<Migration>> {
        let layout = &self.layout;
        ensure!(
            !layout.migration_journal().try_exists()?,
            "migration interrupted; run omega migrate to recover first"
        );
        let old = layout.legacy_plugins_dir();
        let metadata = match std::fs::symlink_metadata(&old) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        ensure!(metadata.is_dir(), "units/ must be a real directory");
        ensure!(
            std::fs::symlink_metadata(layout.plugins_dir())
                .is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound),
            "both units/ and plugins/ exist; resolve the mixed layout before migrating"
        );
        let mut manifests = vec![layout.workspace_manifest(), layout.system_manifest()];
        Migration::manifests(&old, &mut manifests)?;
        if layout.crates_dir().try_exists()? {
            Migration::manifests(&layout.crates_dir(), &mut manifests)?;
        }
        if layout.cargo_config().try_exists()? {
            manifests.push(layout.cargo_config());
        }
        let mut edits = Vec::new();
        for path in manifests {
            let mut edit = FileEdit::read(path.clone())?;
            ensure!(edit.exists(), "missing {}", path.display());
            let mut document: DocumentMut = edit
                .source()
                .parse()
                .with_context(|| format!("parsing {}", path.display()))?;
            let base = if path == layout.cargo_config() {
                &layout.config
            } else {
                path.parent().context("manifest has no parent")?
            };
            if path == layout.workspace_manifest()
                && let Some(members) = document
                    .get("workspace")
                    .and_then(|w| w.get("members"))
                    .and_then(Item::as_array)
            {
                ensure!(
                    !members.iter().any(|v| v
                        .as_str()
                        .is_some_and(|s| s.trim_start_matches("./").starts_with("plugins/"))),
                    "workspace refers to plugins/ while units/ exists; resolve the mixed layout first"
                );
            }
            Migration::cargo_paths(&mut document, base, layout)?;
            edit.replace(document.to_string());
            edits.push(edit);
        }
        Ok(Some(Migration {
            edits: FileEdits::new(edits),
        }))
    }

    pub(crate) fn recover_migration(&self) -> Result<bool> {
        let layout = &self.layout;
        if !layout.migration_journal().try_exists()? {
            return Ok(false);
        }
        let journal = FileEdit::read(layout.migration_journal())?;
        let migration: Migration = serde_json::from_str(journal.source())
            .context("invalid migration journal; inspect .cargo/workspace-migration.json")?;
        for edit in migration.edits.iter() {
            let relative = edit
                .path()
                .strip_prefix(&layout.config)
                .context("migration journal points outside config")?;
            ensure!(
                relative
                    .components()
                    .all(|c| matches!(c, Component::Normal(_))),
                "invalid journal path"
            );
            ensure!(
                edit.path().file_name().is_some_and(|s| s == "Cargo.toml")
                    || edit.path() == layout.cargo_config(),
                "invalid journal file"
            );
        }
        if layout.plugins_dir().try_exists()? {
            ensure!(
                !layout.legacy_plugins_dir().try_exists()?,
                "both units/ and plugins/ exist during recovery; inspect the migration journal"
            );
            Directory::rename_new(&layout.plugins_dir(), &layout.legacy_plugins_dir())?;
        }
        for edit in migration.edits.iter() {
            for parent in edit
                .path()
                .ancestors()
                .skip(1)
                .take_while(|p| *p != layout.config)
            {
                ensure!(
                    std::fs::symlink_metadata(parent)?.is_dir(),
                    "recovery refuses non-directory {}",
                    parent.display()
                );
            }
        }
        migration.edits.restore_all().context(
            "migration recovery incomplete; preserve the journal and resolve the reported file",
        )?;
        Migration::clear(layout)?;
        Ok(true)
    }
}

impl Migration {
    pub(crate) fn paths(&self) -> impl Iterator<Item = &Path> {
        self.edits.iter().filter(|e| e.changed()).map(|e| e.path())
    }

    pub(crate) fn apply(self, workspace: &ConfigWorkspace) -> Result<()> {
        let layout = &workspace.layout;
        for edit in self.edits.iter() {
            edit.check()?;
        }
        ensure!(
            !layout.plugins_dir().try_exists()?,
            "plugins/ appeared during preparation"
        );
        AtomicFile::at(layout.migration_journal()).write(&serde_json::to_vec(&self)?)?;
        let result = self.edits.apply().and_then(|()| {
            Directory::rename_new(&layout.legacy_plugins_dir(), &layout.plugins_dir())
                .map_err(Into::into)
        });
        if let Err(error) = result {
            workspace
                .recover_migration()
                .with_context(|| format!("{error:#}; recovery failed"))?;
            return Err(error.context("workspace migration rolled back"));
        }
        Self::clear(layout)
    }

    fn clear(layout: &Layout) -> Result<()> {
        std::fs::remove_file(layout.migration_journal())?;
        Directory::sync(
            layout
                .migration_journal()
                .parent()
                .context("journal has no parent")?,
        )?;
        Ok(())
    }

    fn manifests(root: &Path, output: &mut Vec<PathBuf>) -> Result<()> {
        ensure!(
            std::fs::symlink_metadata(root)?.is_dir(),
            "{} must be a real directory",
            root.display()
        );
        for entry in std::fs::read_dir(root)? {
            let entry = entry?;
            ensure!(
                !entry.file_type()?.is_symlink(),
                "migration refuses symlink {}",
                entry.path().display()
            );
            if entry.file_type()?.is_dir() {
                let manifest = entry.path().join("Cargo.toml");
                ensure!(
                    manifest.is_file(),
                    "{} has no Cargo.toml; inspect it before migration",
                    entry.path().display()
                );
                output.push(manifest);
            }
        }
        output.sort();
        output.dedup();
        Ok(())
    }

    fn cargo_paths(document: &mut DocumentMut, base: &Path, layout: &Layout) -> Result<()> {
        for name in [
            "dependencies",
            "dev-dependencies",
            "build-dependencies",
            "target",
            "patch",
            "replace",
            "lib",
        ] {
            if let Some(item) = document.get_mut(name) {
                Self::rewrite(item, base, layout)?;
            }
        }
        for name in ["bin", "test", "bench", "example"] {
            if let Some(tables) = document
                .get_mut(name)
                .and_then(Item::as_array_of_tables_mut)
            {
                for table in tables.iter_mut() {
                    if let Some(value) = table.get_mut("path").and_then(Item::as_value_mut) {
                        Self::path(value, base, layout)?;
                    }
                }
            }
        }
        if let Some(workspace) = document
            .get_mut("workspace")
            .and_then(Item::as_table_like_mut)
        {
            for name in ["members", "exclude", "default-members"] {
                if let Some(array) = workspace.get_mut(name).and_then(Item::as_array_mut) {
                    for value in array.iter_mut() {
                        Self::path(value, base, layout)?;
                    }
                }
            }
            if let Some(dependencies) = workspace.get_mut("dependencies") {
                Self::rewrite(dependencies, base, layout)?;
            }
        }
        Ok(())
    }

    fn rewrite(item: &mut Item, base: &Path, layout: &Layout) -> Result<()> {
        if let Some(table) = item.as_table_like_mut() {
            for (key, child) in table.iter_mut() {
                match key.get() {
                    "path" => {
                        if let Some(value) = child.as_value_mut() {
                            Self::path(value, base, layout)?;
                        }
                    }
                    "members" | "exclude" | "default-members" => {
                        if let Some(array) = child.as_array_mut() {
                            for value in array.iter_mut() {
                                Self::path(value, base, layout)?;
                            }
                        }
                    }
                    _ => Self::rewrite(child, base, layout)?,
                }
            }
        }
        Ok(())
    }

    fn path(value: &mut Value, base: &Path, layout: &Layout) -> Result<()> {
        let Some(source) = value.as_str() else {
            return Ok(());
        };
        let mut resolved = PathBuf::new();
        for component in base.join(source).components() {
            match component {
                Component::ParentDir => {
                    resolved.pop();
                }
                Component::CurDir => {}
                other => resolved.push(other),
            }
        }
        if resolved.starts_with(layout.legacy_plugins_dir()) {
            // Both source roots have the same depth; only the component naming
            // the root changes, preserving sibling references and relative paths.
            let parts: Vec<_> = Path::new(source).components().collect();
            let mut current = base.to_path_buf();
            let mut replaced = PathBuf::new();
            for part in parts {
                match part {
                    Component::RootDir => {
                        current = PathBuf::from("/");
                        replaced.push(part);
                    }
                    Component::ParentDir => {
                        current.pop();
                        replaced.push(part);
                    }
                    Component::CurDir => {
                        replaced.push(part);
                    }
                    Component::Normal(name) => {
                        let rename = current == layout.config && name == "units";
                        current.push(name);
                        replaced.push(if rename {
                            std::ffi::OsStr::new("plugins")
                        } else {
                            name
                        });
                    }
                    Component::Prefix(_) => unreachable!("Linux paths have no prefixes"),
                }
            }
            let decor = value.decor().clone();
            *value = Value::from(replaced.to_str().context("non-UTF-8 dependency path")?);
            *value.decor_mut() = decor;
        }
        Ok(())
    }
}
