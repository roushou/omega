use super::dependency::{Dependencies, DependencyTable};
use super::fields::{CargoError, Fields};
use super::{PathPattern, PatternError};
use toml_edit::TableLike;

/// A borrowed `[workspace]` section of a manifest.
#[derive(Clone, Copy)]
pub struct Workspace<'a> {
    pub(super) table: &'a dyn TableLike,
}

impl<'a> Workspace<'a> {
    pub fn resolver(&self) -> Result<Option<&'a str>, CargoError> {
        Fields::string(self.table, "resolver", "workspace.resolver")
    }

    pub fn edition(&self) -> Result<Option<&'a str>, CargoError> {
        self.package_field("edition")
    }

    pub fn version(&self) -> Result<Option<&'a str>, CargoError> {
        self.package_field("version")
    }

    fn package_field(&self, key: &str) -> Result<Option<&'a str>, CargoError> {
        match Fields::table(self.table, "package", "workspace.package")? {
            Some(package) => Fields::string(package, key, &format!("workspace.package.{key}")),
            None => Ok(None),
        }
    }

    /// Declared member patterns. An absent list is empty; malformed lists are errors.
    pub fn members(&self) -> Result<Vec<&'a str>, CargoError> {
        Fields::strings(self.table, "members", "workspace.members")
    }

    pub fn exclude(&self) -> Result<Vec<&'a str>, CargoError> {
        Fields::strings(self.table, "exclude", "workspace.exclude")
    }

    /// Read dependency values without changing their source representation.
    pub fn dependencies(&self) -> Result<Dependencies, CargoError> {
        DependencyTable::read(
            Fields::table(self.table, "dependencies", "workspace.dependencies")?,
            "workspace.dependencies",
        )
    }

    /// Expand members against `root`, apply exclusions, then sort and deduplicate.
    /// This operation reads directories; the other accessors perform no I/O.
    pub fn member_dirs(
        &self,
        root: &std::path::Path,
    ) -> Result<Vec<std::path::PathBuf>, MembersError> {
        let mut excluded = Vec::new();
        for pattern in self.exclude()? {
            excluded.extend(PathPattern::new(pattern).expand(root)?);
        }
        let mut dirs = Vec::new();
        for pattern in self.members()? {
            for dir in PathPattern::new(pattern).expand(root)? {
                if !excluded.contains(&dir) {
                    dirs.push(dir);
                }
            }
        }
        dirs.sort();
        dirs.dedup();
        Ok(dirs)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum MembersError {
    #[error(transparent)]
    Field(#[from] CargoError),
    #[error(transparent)]
    Pattern(#[from] PatternError),
}

impl std::fmt::Debug for Workspace<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_map().entries(self.table.iter()).finish()
    }
}
