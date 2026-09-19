use super::PathPattern;
use super::dependency::DependencyTable;
use super::fields::Fields;
use super::{CargoError, Dependencies, Dependency, Inherited, Package, Workspace};
use crate::{Layout, TomlError, TomlFile, TomlSchema};
use omega_proto::PluginName;
use toml_edit::{Array, DocumentMut, Item, Table, value};

/// Which manifest to address through [`Layout::file`].
#[derive(Debug, Clone, Copy)]
pub enum CargoSlot<'a> {
    Workspace,
    Plugin(&'a PluginName),
    System,
    /// A member directory, absolute or relative to the config root.
    Member(&'a std::path::Path),
}

/// A `Cargo.toml` that retains comments, formatting, and unknown fields.
///
/// Parsing checks TOML syntax. Accessors validate the fields they read, allowing
/// unrelated Cargo features to remain opaque. Fallible edits leave the document
/// unchanged on error. No operation on this type reads or writes files.
///
/// ```
/// use omega_host::cargo::Manifest;
/// let mut manifest = "[workspace]\nmembers = [\"system\"]\n".parse::<Manifest>()?;
/// manifest.ensure_member("plugins/clock", "plugins/*")?;
/// assert_eq!(manifest.workspace()?.unwrap().members()?, ["system", "plugins/*"]);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug, Default, Clone)]
pub struct Manifest {
    document: DocumentMut,
}

impl std::str::FromStr for Manifest {
    type Err = TomlError;

    fn from_str(source: &str) -> Result<Self, Self::Err> {
        Ok(Self {
            document: source.parse().map_err(|source| TomlError::Decode {
                kind: Self::KIND,
                source: Box::new(source),
            })?,
        })
    }
}

impl Manifest {
    pub fn package(&self) -> Result<Option<Package<'_>>, CargoError> {
        Ok(
            Fields::table(self.document.as_table(), "package", "package")?
                .map(|table| Package { table }),
        )
    }

    pub fn workspace(&self) -> Result<Option<Workspace<'_>>, CargoError> {
        Ok(
            Fields::table(self.document.as_table(), "workspace", "workspace")?
                .map(|table| Workspace { table }),
        )
    }

    pub fn dependencies(&self) -> Result<Dependencies, CargoError> {
        DependencyTable::read(
            Fields::table(self.document.as_table(), "dependencies", "dependencies")?,
            "dependencies",
        )
    }

    /// Construct a new package manifest. Edition inheritance is explicit.
    pub fn new_package(
        name: &str,
        version: &str,
        edition: Inherited<'_>,
        dependencies: &Dependencies,
    ) -> Result<Self, CargoError> {
        let mut document = DocumentMut::new();
        let mut package = Table::new();
        package.insert("name", value(name));
        package.insert("version", value(version));
        match edition {
            Inherited::Value(edition) => {
                package.insert("edition", value(edition));
            }
            Inherited::Workspace => {
                let mut inherited = toml_edit::InlineTable::new();
                inherited.insert("workspace", true.into());
                inherited.fmt();
                package.insert("edition", value(inherited));
            }
        }
        document.insert("package", Item::Table(package));
        if !dependencies.is_empty() {
            document.insert(
                "dependencies",
                Item::Table(DependencyTable::table(dependencies)?),
            );
        }
        Ok(Self { document })
    }

    /// Mark this package as an executable command host for Omega discovery.
    /// Retain unrelated package metadata. Malformed metadata leaves the document unchanged.
    pub fn set_command_host(&mut self) -> Result<(), CargoError> {
        self.edit(|document| {
            let mut table = document
                .get_mut("package")
                .and_then(Item::as_table_like_mut)
                .ok_or_else(|| CargoError::new("package", "missing or not a table"))?;
            for (key, path) in [
                ("metadata", "package.metadata"),
                ("omega", "package.metadata.omega"),
            ] {
                let mut implicit = Table::new();
                implicit.set_implicit(true);
                table = table
                    .entry(key)
                    .or_insert(Item::Table(implicit))
                    .as_table_like_mut()
                    .ok_or_else(|| CargoError::new(path, "must be a table"))?;
            }
            if let Some(kind) = table.get_mut("kind") {
                Fields::replace(kind, value("command-host"));
            } else {
                table.insert("kind", value("command-host"));
            }
            Ok(())
        })
    }

    /// Construct a virtual workspace with caller-selected members and defaults.
    pub fn new_workspace(
        resolver: &str,
        edition: &str,
        members: &[&str],
        dependencies: &Dependencies,
    ) -> Result<Self, CargoError> {
        let mut workspace = Table::new();
        workspace.insert("resolver", value(resolver));
        workspace.insert("members", value(members.iter().copied().collect::<Array>()));

        let mut package = Table::new();
        package.insert("edition", value(edition));
        workspace.insert("package", Item::Table(package));
        workspace.insert(
            "dependencies",
            Item::Table(DependencyTable::table(dependencies)?),
        );

        let mut document = DocumentMut::new();
        document.insert("workspace", Item::Table(workspace));
        Ok(Self { document })
    }

    /// Set only the release profile's strip and LTO options.
    pub fn set_release_profile(&mut self, strip: bool, lto: &str) -> Result<(), CargoError> {
        self.edit(|document| {
            let mut implicit = Table::new();
            implicit.set_implicit(true);
            let profiles = document
                .entry("profile")
                .or_insert(Item::Table(implicit))
                .as_table_like_mut()
                .ok_or_else(|| CargoError::new("profile", "must be a table"))?;
            let release = profiles
                .entry("release")
                .or_insert(Item::Table(Table::new()))
                .as_table_like_mut()
                .ok_or_else(|| CargoError::new("profile.release", "must be a table"))?;
            for (key, new) in [("strip", value(strip)), ("lto", value(lto))] {
                if let Some(existing) = release.get_mut(key) {
                    Fields::replace(existing, new);
                } else {
                    release.insert(key, new);
                }
            }
            Ok(())
        })
    }

    /// Fill missing workspace edition and dependencies. Existing versions and
    /// options are retained; conflicting aliases and recursive inheritance fail.
    pub fn ensure_workspace_defaults(
        &mut self,
        edition: &str,
        defaults: &Dependencies,
    ) -> Result<(), CargoError> {
        self.edit(|document| {
            let root = document
                .get_mut("workspace")
                .and_then(Item::as_table_like_mut)
                .ok_or_else(|| CargoError::new("workspace", "missing or not a table"))?;
            let package = root
                .entry("package")
                .or_insert(Item::Table(Table::new()))
                .as_table_like_mut()
                .ok_or_else(|| CargoError::new("workspace.package", "must be a table"))?;
            if Fields::string(package, "edition", "workspace.package.edition")?.is_none() {
                package.insert("edition", value(edition));
            }
            let dependencies = root
                .entry("dependencies")
                .or_insert(Item::Table(Table::new()))
                .as_table_like_mut()
                .ok_or_else(|| CargoError::new("workspace.dependencies", "must be a table"))?;
            let existing = DependencyTable::read(Some(dependencies), "workspace.dependencies")?;
            for (name, expected) in defaults.iter() {
                if let Some(actual) = existing.get(name) {
                    let expected_package = expected.package().unwrap_or(name);
                    let actual_package = actual.package().unwrap_or(name);
                    if actual_package != expected_package {
                        return Err(CargoError::new(
                            format!("workspace.dependencies.{name}"),
                            format!("refers to {actual_package}, expected {expected_package}"),
                        ));
                    }
                    if matches!(actual, Dependency::Detailed(detail) if detail.workspace.is_some())
                    {
                        return Err(CargoError::new(
                            format!("workspace.dependencies.{name}"),
                            "workspace dependency cannot itself inherit",
                        ));
                    }
                } else {
                    dependencies.insert(name, DependencyTable::item(expected)?);
                }
            }
            Ok(())
        })
    }

    /// Ensure a member is covered by the workspace. Existing matching patterns
    /// are retained. The caller selects the pattern to append; exclusions fail.
    pub fn ensure_member(&mut self, relative: &str, pattern: &str) -> Result<(), CargoError> {
        if !PathPattern::new(pattern).matches(relative) {
            return Err(CargoError::new(
                "workspace.members",
                "the supplied pattern does not cover the member",
            ));
        }
        let workspace = self
            .workspace()?
            .ok_or_else(|| CargoError::new("workspace", "is required"))?;
        for excluded in workspace.exclude()? {
            if PathPattern::new(excluded).matches(relative) {
                return Err(CargoError::new(
                    "workspace.exclude",
                    format!("{relative} is excluded by {excluded}; update the exclusion first"),
                ));
            }
        }
        if workspace
            .members()?
            .iter()
            .any(|existing| PathPattern::new(existing).matches(relative))
        {
            return Ok(());
        }
        self.edit(|document| {
            let workspace = document
                .get_mut("workspace")
                .and_then(Item::as_table_like_mut)
                .expect("validated workspace");
            let members = workspace
                .entry("members")
                .or_insert(value(Array::new()))
                .as_array_mut()
                .expect("validated members");
            members.push(pattern);
            Ok(())
        })
    }

    /// Add a path dependency unless its Rust import name is occupied. An existing
    /// identical path is retained with its features and other options.
    pub fn ensure_path_dependency(
        &mut self,
        name: &str,
        dependency: &Dependency,
    ) -> Result<(), CargoError> {
        if dependency.path().is_none() {
            return Err(CargoError::new(
                format!("dependencies.{name}"),
                "expected a path dependency",
            ));
        }
        for (alias, existing) in self.dependencies()?.iter() {
            if alias.replace('-', "_") == name.replace('-', "_") {
                if alias == name
                    && existing.path() == dependency.path()
                    && existing.package().is_none()
                    && existing.repository().is_none()
                    && !matches!(existing, Dependency::Detailed(detail) if detail.workspace.is_some())
                {
                    return Ok(());
                }
                return Err(CargoError::new(
                    format!("dependencies.{alias}"),
                    format!(
                        "dependency already occupies the Rust name {}",
                        name.replace('-', "_")
                    ),
                ));
            }
        }
        let entry = DependencyTable::item(dependency)?;
        self.edit(|document| {
            let dependencies = document
                .entry("dependencies")
                .or_insert(Item::Table(Table::new()))
                .as_table_like_mut()
                .ok_or_else(|| CargoError::new("dependencies", "must be a table"))?;
            dependencies.insert(name, entry);
            Ok(())
        })
    }

    /// Update the registry requirement for selected workspace dependencies.
    /// Path, Git, and inherited declarations are rejected; unrelated options remain.
    pub fn require_versions(&mut self, names: &[&str], version: &str) -> Result<(), CargoError> {
        self.edit(|document| {
            let dependencies = document
                .get_mut("workspace")
                .and_then(|workspace| workspace.get_mut("dependencies"))
                .and_then(Item::as_table_like_mut)
                .ok_or_else(|| CargoError::new("workspace.dependencies", "missing or not a table"))?;

            for name in names {
                let field = format!("workspace.dependencies.{name}");
                let dependency = dependencies.get_mut(name)
                    .ok_or_else(|| CargoError::new(&field, "missing workspace dependency"))?;

                if dependency.as_str().is_some() {
                    Fields::replace(dependency, value(version));
                    continue;
                }

                let table = dependency.as_table_like_mut()
                    .ok_or_else(|| CargoError::new(&field, "invalid dependency"))?;

                if table.get("git").is_some() || table.get("path").is_some() || table.get("workspace").is_some() {
                    return Err(CargoError::new(&field,
                        "has an explicit source or inheritance; use a registry dependency before linking"));
                }

                if let Some(existing) = table.get_mut("version") {
                    Fields::replace(existing, value(version));
                } else {
                    table.insert("version", value(version));
                }
            }

            Ok(())
        })
    }

    /// Remove the package dependency section.
    pub fn clear_dependencies(&mut self) {
        self.document.remove("dependencies");
    }

    fn edit(
        &mut self,
        edit: impl FnOnce(&mut DocumentMut) -> Result<(), CargoError>,
    ) -> Result<(), CargoError> {
        let mut document = self.document.clone();
        edit(&mut document)?;
        self.document = document;
        Ok(())
    }
}

impl std::fmt::Display for Manifest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.document.fmt(f)
    }
}

impl TomlSchema for Manifest {
    const KIND: &'static str = "cargo manifest";
    type Key<'a> = CargoSlot<'a>;

    fn decode(source: &str) -> Result<Self, TomlError> {
        source.parse()
    }
    fn encode(&self) -> Result<String, TomlError> {
        Ok(self.to_string())
    }

    fn locate(layout: &Layout, key: Self::Key<'_>) -> TomlFile<Self> {
        TomlFile::at(match key {
            CargoSlot::Workspace => layout.workspace_manifest(),
            CargoSlot::Plugin(name) => layout.plugin_crate_manifest(name),
            CargoSlot::System => layout.system_manifest(),
            CargoSlot::Member(directory) => layout.member_manifest(directory),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_host_metadata_preserves_other_fields_and_failed_edits_are_atomic() {
        let source = "[package]\nname = 'audio'\n[package.metadata.other]\nkeep = true # keep\n[package.metadata.omega]\nkind = 'plugin' # role\n";
        let mut manifest = source.parse::<Manifest>().unwrap();
        manifest.set_command_host().unwrap();
        assert_eq!(
            manifest.package().unwrap().unwrap().omega_kind().unwrap(),
            Some("command-host")
        );
        assert!(manifest.to_string().contains("keep = true # keep"));
        assert!(manifest.to_string().contains("# role"));
        let source = "[package]\nname = 'audio'\nmetadata = false\n";
        let mut manifest = source.parse::<Manifest>().unwrap();
        assert!(manifest.set_command_host().is_err());
        assert_eq!(manifest.to_string(), source);
    }

    #[test]
    fn borrowed_package_fields_support_inheritance_and_cargo_defaults() {
        let source = "# package\n[package]\nname = 'desktop'\nversion.workspace = true\nedition = { workspace = true }\n[workspace]\nmembers = []\n";
        let manifest = source.parse::<Manifest>().unwrap();
        let package = manifest.package().unwrap().unwrap();

        assert_eq!(package.name().unwrap(), "desktop");
        assert_eq!(package.version().unwrap(), Some(Inherited::Workspace));
        assert_eq!(package.edition().unwrap(), Some(Inherited::Workspace));
        assert!(
            manifest
                .workspace()
                .unwrap()
                .unwrap()
                .members()
                .unwrap()
                .is_empty()
        );
        assert_eq!(manifest.to_string(), source);

        let manifest = "[package]\nname = 'minimal'\n".parse::<Manifest>().unwrap();
        assert_eq!(
            manifest.package().unwrap().unwrap().version().unwrap(),
            None
        );
        assert_eq!(
            manifest.package().unwrap().unwrap().edition().unwrap(),
            None
        );
    }

    #[test]
    fn malformed_fields_are_reported_at_the_accessed_field() {
        for declaration in [
            "version = false",
            "version = { workspace = false }",
            "version = { workspace = true, extra = 1 }",
        ] {
            let manifest = format!("[package]\nname = 'desktop'\n{declaration}\n")
                .parse::<Manifest>()
                .unwrap();
            let package = manifest.package().unwrap().unwrap();
            assert_eq!(package.name().unwrap(), "desktop");
            assert_eq!(package.version().unwrap_err().field, "package.version");
        }

        let manifest = "[workspace]\nmembers = ['system', 42]\n"
            .parse::<Manifest>()
            .unwrap();
        assert_eq!(
            manifest
                .workspace()
                .unwrap()
                .unwrap()
                .members()
                .unwrap_err()
                .field,
            "workspace.members"
        );
    }

    #[test]
    fn version_edits_preserve_all_dependency_shapes_and_surrounding_source() {
        let source = "# workspace\n[workspace.dependencies]\nplain = '1' # plain\ninline = { version = '1', features = ['derive'] } # options\ndotted.version = '1' # dotted\n\n[workspace.dependencies.expanded]\nversion = '1' # expanded\noptional = true\n";
        let mut manifest = source.parse::<Manifest>().unwrap();
        let dependencies = manifest
            .workspace()
            .unwrap()
            .unwrap()
            .dependencies()
            .unwrap();
        for name in ["plain", "inline", "dotted", "expanded"] {
            assert_eq!(dependencies.get(name).unwrap().version(), Some("1"));
        }

        manifest
            .require_versions(&["plain", "inline", "dotted", "expanded"], "2")
            .unwrap();
        assert_eq!(manifest.to_string(), source.replace("'1'", "\"2\""));
    }

    #[test]
    fn rejected_edits_do_not_leave_partial_changes() {
        let source = "[workspace]\nmembers = ['system', 42]\n[workspace.dependencies]\nfirst = '1'\nlast = { path = '../last' }\n";
        let mut manifest = source.parse::<Manifest>().unwrap();
        assert!(manifest.require_versions(&["first", "last"], "2").is_err());
        assert_eq!(manifest.to_string(), source);
        assert!(
            manifest
                .ensure_member("plugins/clock", "plugins/*")
                .is_err()
        );
        assert_eq!(manifest.to_string(), source);

        let defaults = Dependencies::from_iter([
            ("added", Dependency::registry("1", &[])),
            ("last", Dependency::renamed("different", "1", &[])),
        ]);
        assert!(
            manifest
                .ensure_workspace_defaults("2024", &defaults)
                .is_err()
        );
        assert_eq!(manifest.to_string(), source);
    }

    #[test]
    fn existing_path_dependencies_keep_options_and_collisions_fail() {
        let source =
            "[dependencies]\nshared-types = { path = '../shared', features = ['ui'] } # keep\n";
        let mut manifest = source.parse::<Manifest>().unwrap();
        let dependency = Dependency::local("../shared", &[]);
        manifest
            .ensure_path_dependency("shared-types", &dependency)
            .unwrap();
        assert_eq!(manifest.to_string(), source);
        assert!(
            manifest
                .ensure_path_dependency("shared_types", &dependency)
                .is_err()
        );
        assert_eq!(manifest.to_string(), source);
    }

    #[test]
    fn member_edits_support_inline_workspaces_and_validate_the_whole_list() {
        let mut manifest = "workspace = { members = ['system'] } # workspace\n"
            .parse::<Manifest>()
            .unwrap();
        manifest
            .ensure_member("plugins/clock", "plugins/*")
            .unwrap();
        assert!(manifest.to_string().ends_with("# workspace\n"));
        assert_eq!(
            manifest.workspace().unwrap().unwrap().members().unwrap(),
            ["system", "plugins/*"]
        );
        let source = manifest.to_string();
        manifest
            .ensure_member("plugins/audio", "plugins/*")
            .unwrap();
        assert_eq!(manifest.to_string(), source);
        assert!(
            manifest
                .ensure_member("crates/shared", "plugins/*")
                .is_err()
        );
        assert_eq!(manifest.to_string(), source);

        let source = "[workspace]\nmembers = ['plugins/*', 42]\n";
        let mut manifest = source.parse::<Manifest>().unwrap();
        assert!(
            manifest
                .ensure_member("plugins/clock", "plugins/*")
                .is_err()
        );
        assert_eq!(manifest.to_string(), source);
    }
}
