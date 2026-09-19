use super::fields::{CargoError, Fields};
use toml_edit::TableLike;

/// A literal package field or an explicit request to inherit it from the workspace.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Inherited<'a> {
    Value(&'a str),
    Workspace,
}

/// A borrowed `[package]` section. Reading one field does not parse unrelated fields.
#[derive(Clone, Copy)]
pub struct Package<'a> {
    pub(super) table: &'a dyn TableLike,
}

impl<'a> Package<'a> {
    /// The package name. Missing or non-string names are errors.
    pub fn name(&self) -> Result<&'a str, CargoError> {
        Fields::string(self.table, "name", "package.name")?
            .ok_or_else(|| CargoError::new("package.name", "is required"))
    }

    /// Optional Omega executable role from package metadata.
    pub fn omega_kind(&self) -> Result<Option<&'a str>, CargoError> {
        let Some(metadata) = Fields::table(self.table, "metadata", "package.metadata")? else {
            return Ok(None);
        };
        let Some(omega) = Fields::table(metadata, "omega", "package.metadata.omega")? else {
            return Ok(None);
        };
        Fields::string(omega, "kind", "package.metadata.omega.kind")
    }

    /// The declared version; `None` means Cargo's default applies.
    pub fn version(&self) -> Result<Option<Inherited<'a>>, CargoError> {
        self.inherited("version")
    }

    /// The declared edition; `None` means Cargo's default applies.
    pub fn edition(&self) -> Result<Option<Inherited<'a>>, CargoError> {
        self.inherited("edition")
    }

    fn inherited(&self, key: &str) -> Result<Option<Inherited<'a>>, CargoError> {
        let Some(item) = self.table.get(key) else {
            return Ok(None);
        };
        if let Some(value) = item.as_str() {
            return Ok(Some(Inherited::Value(value)));
        }
        if item.as_table_like().is_some_and(|table| {
            table.len() == 1
                && table.get("workspace").and_then(|value| value.as_bool()) == Some(true)
        }) {
            return Ok(Some(Inherited::Workspace));
        }
        Err(CargoError::new(
            format!("package.{key}"),
            "must be a string or { workspace = true }",
        ))
    }
}

impl std::fmt::Debug for Package<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_map().entries(self.table.iter()).finish()
    }
}
