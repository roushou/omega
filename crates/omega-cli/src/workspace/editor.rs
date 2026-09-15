use anyhow::{Context, Result, bail, ensure};
use omega_host::Toml;
use omega_host::workspace::PathPattern;
use omega_host::workspace::cargo::{CargoManifest, Dependency};
use toml_edit::{Array, DocumentMut, Item, Table, value};

/// Edits only the Cargo entries Omega owns, retaining the surrounding source.
#[derive(Debug)]
pub(crate) struct CargoEditor {
    document: DocumentMut,
}

impl CargoEditor {
    pub(crate) fn parse(source: &str) -> Result<Self> {
        Ok(Self {
            document: source.parse().context("invalid Cargo document")?,
        })
    }

    pub(crate) fn package_name(&self) -> Result<&str> {
        self.document
            .get("package")
            .and_then(|p| p.get("name"))
            .and_then(Item::as_str)
            .context("Cargo manifest has no package.name")
    }

    pub(crate) fn workspace(&mut self, defaults: &CargoManifest) -> Result<()> {
        let root = self
            .document
            .get_mut("workspace")
            .and_then(Item::as_table_mut)
            .context("the config manifest has no [workspace]")?;
        let package = root
            .entry("package")
            .or_insert(Item::Table(Table::new()))
            .as_table_like_mut()
            .context("workspace.package must be a table")?;
        if let Some(edition) = package.get("edition") {
            ensure!(
                edition.as_str().is_some(),
                "workspace.package.edition must be a string"
            );
        } else {
            package.insert(
                "edition",
                value(
                    defaults
                        .workspace
                        .as_ref()
                        .and_then(|w| w.package.as_ref())
                        .and_then(|p| p.edition.as_deref())
                        .context("scaffold has no edition")?,
                ),
            );
        }
        let dependencies = root
            .entry("dependencies")
            .or_insert(Item::Table(Table::new()))
            .as_table_like_mut()
            .context("workspace.dependencies must be a table")?;
        let generated = Self::parse(&Toml::encode(defaults)?)?;
        let expected = generated.document["workspace"]["dependencies"]
            .as_table_like()
            .unwrap();
        for (name, entry) in expected.iter() {
            if let Some(existing) = dependencies.get(name) {
                ensure!(
                    existing.as_str().is_some() || existing.as_table_like().is_some(),
                    "invalid dependency {name}"
                );
                let expected_package = entry.get("package").and_then(Item::as_str).unwrap_or(name);
                let actual_package = existing
                    .get("package")
                    .and_then(Item::as_str)
                    .unwrap_or(name);
                ensure!(
                    actual_package == expected_package,
                    "workspace dependency {name} refers to {actual_package}, expected {expected_package}"
                );
                ensure!(
                    existing.get("workspace").is_none(),
                    "workspace dependency {name} cannot itself inherit"
                );
            } else {
                dependencies.insert(name, entry.clone());
            }
        }
        Ok(())
    }

    pub(crate) fn member(&mut self, relative: &str, default_pattern: &str) -> Result<()> {
        let workspace = self
            .document
            .get_mut("workspace")
            .and_then(Item::as_table_mut)
            .context("the config manifest has no [workspace]")?;
        if let Some(exclude) = workspace.get("exclude") {
            let exclude = exclude
                .as_array()
                .context("workspace.exclude must be an array")?;
            for pattern in exclude.iter() {
                let pattern = pattern
                    .as_str()
                    .context("workspace.exclude must contain strings")?;
                ensure!(
                    !PathPattern::new(pattern).matches(relative),
                    "{relative} is excluded by workspace.exclude ({pattern}); update the exclusion first"
                );
            }
        }
        let members = workspace
            .entry("members")
            .or_insert(value(Array::new()))
            .as_array_mut()
            .context("workspace.members must be an array")?;
        for pattern in members.iter() {
            let pattern = pattern
                .as_str()
                .context("workspace.members must contain strings")?;
            if PathPattern::new(pattern).matches(relative) {
                return Ok(());
            }
        }
        members.push(default_pattern);
        Ok(())
    }

    pub(crate) fn dependency(&mut self, name: &str, dependency: &Dependency) -> Result<()> {
        let table = self
            .document
            .entry("dependencies")
            .or_insert(Item::Table(Table::new()))
            .as_table_like_mut()
            .context("dependencies must be a table")?;
        for (alias, _) in table.iter() {
            if alias.replace('-', "_") == name.replace('-', "_") {
                let existing = table.get(alias).expect("entry being iterated exists");
                if alias == name
                    && existing.get("path").and_then(Item::as_str) == dependency.path()
                    && existing.get("package").is_none()
                    && existing.get("git").is_none()
                    && existing.get("workspace").is_none()
                {
                    return Ok(());
                }
                bail!(
                    "dependency {alias} already occupies the Rust name {}; choose a different plugin name",
                    name.replace('-', "_")
                );
            }
        }
        let mut generated = CargoManifest::default();
        generated.dependencies.insert(name, dependency.clone());
        let generated = Self::parse(&Toml::encode(&generated)?)?;
        table.insert(name, generated.document["dependencies"][name].clone());
        Ok(())
    }

    pub(crate) fn link(&mut self, patch: &CargoEditor, names: &[&str]) -> Result<()> {
        let sources = self
            .document
            .entry("patch")
            .or_insert(Item::Table(Table::new()))
            .as_table_like_mut()
            .context("patch must be a table")?;
        let registry = sources
            .entry("crates-io")
            .or_insert(Item::Table(Table::new()))
            .as_table_like_mut()
            .context("patch.crates-io must be a table")?;
        for name in names {
            if let Some(entry) = patch
                .document
                .get("patch")
                .and_then(|p| p.get("crates-io"))
                .and_then(|p| p.get(name))
            {
                registry.insert(name, entry.clone());
            } else {
                registry.remove(name);
            }
        }
        if registry.is_empty() {
            sources.remove("crates-io");
        }
        if sources.is_empty() {
            self.document.remove("patch");
        }
        Ok(())
    }

    pub(crate) fn has_workspace_dependency(&self, name: &str) -> bool {
        self.document
            .get("workspace")
            .and_then(|w| w.get("dependencies"))
            .and_then(|d| d.get(name))
            .is_some()
    }

    pub(crate) fn require(&mut self, names: &[&str], version: &str) -> Result<()> {
        let dependencies = self
            .document
            .get_mut("workspace")
            .and_then(|w| w.get_mut("dependencies"))
            .and_then(Item::as_table_like_mut)
            .context("missing workspace.dependencies")?;
        for name in names {
            let dependency = dependencies
                .get_mut(name)
                .with_context(|| format!("missing workspace dependency {name}"))?;
            if dependency.as_str().is_some() {
                let decor = dependency.as_value().unwrap().decor().clone();
                *dependency = value(version);
                *dependency.as_value_mut().unwrap().decor_mut() = decor;
            } else {
                let table = dependency
                    .as_table_like_mut()
                    .context("invalid dependency")?;
                ensure!(
                    table.get("git").is_none() && table.get("path").is_none(),
                    "{name} has an explicit source; use a registry dependency before linking"
                );
                table.insert("version", value(version));
            }
        }
        Ok(())
    }

    pub(crate) fn finish(self) -> String {
        self.document.to_string()
    }
}
