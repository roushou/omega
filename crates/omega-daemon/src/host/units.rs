//! Which units exist.
//!
//! The workspace manifest is the source of truth: its `members` globs name
//! the unit crates under `units/`. Units are derived from it, so there is no
//! separate registry that can drift out of sync with the crates.

use crate::host::cargo::{CargoManifest, CargoSlot};
use crate::host::error::UnitsError;
use omega_proto::Layout;
use omega_proto::UnitName;

/// The units of a config workspace, in deterministic (sorted) order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Units(Vec<UnitName>);

impl Units {
    /// Read the workspace manifest and derive its units: member directories
    /// directly under `units/`, minus `exclude`.
    pub fn discover(layout: &Layout) -> Result<Self, UnitsError> {
        let manifest = layout.file::<CargoManifest>(CargoSlot::Workspace).read()?;
        let workspace = manifest.workspace.unwrap_or_default();
        let units_dir = layout.units_dir();

        let mut names = Vec::new();
        for dir in workspace.member_dirs(&layout.config)? {
            // Only direct children of `units/` are units; other members (a
            // shared `lib/` crate, say) are workspace members but not units.
            if dir.parent() != Some(units_dir.as_path()) {
                continue;
            }
            let name = dir
                .file_name()
                .and_then(|n| n.to_str())
                .ok_or_else(|| UnitsError::UnnamedMember(dir.clone()))?;
            names.push(UnitName::parse(name)?);
        }

        names.sort();
        names.dedup();
        Ok(Self(names))
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn iter(&self) -> impl Iterator<Item = &UnitName> {
        self.0.iter()
    }
}

impl IntoIterator for Units {
    type Item = UnitName;
    type IntoIter = std::vec::IntoIter<UnitName>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a> IntoIterator for &'a Units {
    type Item = &'a UnitName;
    type IntoIter = std::slice::Iter<'a, UnitName>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}
