use super::dependency::DependencyTable;
use super::fields::Fields;
use super::{CargoError, Dependencies};
use crate::{Layout, TomlError, TomlFile, TomlSchema};
use toml_edit::{DocumentMut, Item, Table};

/// A `.cargo/config.toml` retaining source formatting and unknown Cargo settings.
/// Patch edits affect only the selected registry and package names. Parsing and
/// editing perform no I/O; persist through [`Layout::file`].
///
/// ```
/// use omega_host::cargo::{Config, Dependencies, Dependency};
/// let mut config = "[build]\njobs = 2\n".parse::<Config>()?;
/// let patches = Dependencies::from_iter([("example", Dependency::local("../example", &[]))]);
/// config.update_patches(Config::REGISTRY, &["example"], &patches)?;
/// assert_eq!(config.patches(Config::REGISTRY)?.get("example").unwrap().path(), Some("../example"));
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug, Default, Clone)]
pub struct Config {
    document: DocumentMut,
}

impl std::str::FromStr for Config {
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

impl Config {
    pub const REGISTRY: &'static str = "crates-io";

    /// Read dependency values for a registry. Missing patch tables are empty;
    /// malformed tables and declarations are errors.
    pub fn patches(&self, registry: &str) -> Result<Dependencies, CargoError> {
        let Some(patch) = Fields::table(self.document.as_table(), "patch", "patch")? else {
            return Ok(Dependencies::new());
        };
        DependencyTable::read(
            Fields::table(patch, registry, &format!("patch.{registry}"))?,
            &format!("patch.{registry}"),
        )
    }

    /// Replace or remove only `names` in a registry's patch table. A name absent
    /// from `replacements` is removed. Other entries and registries are retained.
    /// On error the whole document remains unchanged.
    pub fn update_patches(
        &mut self,
        registry: &str,
        names: &[&str],
        replacements: &Dependencies,
    ) -> Result<(), CargoError> {
        let mut document = self.document.clone();
        let patch = document
            .entry("patch")
            .or_insert(Item::Table(Table::new()))
            .as_table_like_mut()
            .ok_or_else(|| CargoError::new("patch", "must be a table"))?;
        let entries = patch
            .entry(registry)
            .or_insert(Item::Table(Table::new()))
            .as_table_like_mut()
            .ok_or_else(|| CargoError::new(format!("patch.{registry}"), "must be a table"))?;
        for name in names {
            if let Some(dependency) = replacements.get(name) {
                let replacement = DependencyTable::item(dependency)?;
                if let Some(existing) = entries.get_mut(name) {
                    Fields::replace(existing, replacement);
                } else {
                    entries.insert(name, replacement);
                }
            } else {
                entries.remove(name);
            }
        }
        if entries.is_empty() {
            patch.remove(registry);
        }
        if patch.is_empty() {
            document.remove("patch");
        }
        self.document = document;
        Ok(())
    }
}

impl std::fmt::Display for Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.document.fmt(f)
    }
}

impl TomlSchema for Config {
    const KIND: &'static str = "cargo config";
    type Key<'a> = ();

    fn decode(source: &str) -> Result<Self, TomlError> {
        source.parse()
    }
    fn encode(&self) -> Result<String, TomlError> {
        Ok(self.to_string())
    }

    fn locate(layout: &Layout, _: ()) -> TomlFile<Self> {
        TomlFile::at(layout.cargo_config())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cargo::Dependency;

    #[test]
    fn scoped_patch_updates_preserve_other_registries_and_options() {
        let source = "# compiler\n[build]\njobs = 2\n\n[patch.crates-io]\nselected = { path = '/old' } # selected\nother = { path = '/other' } # unrelated\n\n[patch.custom]\nselected = { path = '/custom' }\n";
        let mut config = source.parse::<Config>().unwrap();
        assert_eq!(config.to_string(), source);
        config
            .update_patches(
                Config::REGISTRY,
                &["selected"],
                &Dependencies::from_iter([("selected", Dependency::local("/new", &[]))]),
            )
            .unwrap();
        assert_eq!(
            config.to_string(),
            source.replace("{ path = '/old' }", "{ path = \"/new\" }")
        );
        config
            .update_patches(Config::REGISTRY, &["selected"], &Dependencies::new())
            .unwrap();
        assert!(
            config
                .patches(Config::REGISTRY)
                .unwrap()
                .get("other")
                .is_some()
        );
        assert!(
            config
                .patches(Config::REGISTRY)
                .unwrap()
                .get("selected")
                .is_none()
        );
        assert_eq!(
            config
                .patches("custom")
                .unwrap()
                .get("selected")
                .unwrap()
                .path(),
            Some("/custom")
        );
    }

    #[test]
    fn malformed_patch_tables_fail_without_mutation() {
        for source in ["patch = 1\n", "[patch]\ncrates-io = 1\n"] {
            let mut config = source.parse::<Config>().unwrap();
            assert!(config.patches(Config::REGISTRY).is_err());
            assert!(
                config
                    .update_patches(Config::REGISTRY, &["selected"], &Dependencies::new())
                    .is_err()
            );
            assert_eq!(config.to_string(), source);
        }
    }

    #[test]
    fn removing_last_owned_patch_does_not_create_empty_sections() {
        let mut config = Config::default();
        config
            .update_patches(Config::REGISTRY, &["selected"], &Dependencies::new())
            .unwrap();
        assert_eq!(config.to_string(), "");
        config
            .update_patches(
                Config::REGISTRY,
                &["selected"],
                &Dependencies::from_iter([("selected", Dependency::local("/source", &[]))]),
            )
            .unwrap();
        config
            .update_patches(Config::REGISTRY, &["selected"], &Dependencies::new())
            .unwrap();
        assert_eq!(config.to_string(), "");
    }
}
