//! Package and workspace manifests, including member expansion.
use super::{Dependencies, Package, Profile, WorkspacePackage};
use crate::Table;
use crate::workspace::{PathPattern, PatternError};
use crate::{Layout, TomlFile, TomlSchema};
use omega_proto::UnitName;
use serde::{Deserialize, Serialize};

/// Which `Cargo.toml` in the config workspace.
#[derive(Debug, Clone, Copy)]
pub enum CargoSlot<'a> {
    /// `~/.config/omega/Cargo.toml`.
    Workspace,
    /// `~/.config/omega/plugins/<name>/Cargo.toml`.
    Unit(&'a UnitName),
    /// `~/.config/omega/system/Cargo.toml` — the configuration plane.
    System,
}

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct CargoManifest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub package: Option<Package>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace: Option<Workspace>,
    #[serde(default, skip_serializing_if = "Dependencies::is_empty")]
    pub dependencies: Dependencies,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile: Option<Profile>,
    /// Everything omega does not model. Kept so writing a manifest back does
    /// not delete the parts of it we never understood.
    #[serde(flatten)]
    pub rest: Table<toml::Value>,
}

impl TomlSchema for CargoManifest {
    const KIND: &'static str = "cargo manifest";
    const INLINE_ENTRIES: &'static [&'static str] = &[
        "dependencies",
        "dev-dependencies",
        "build-dependencies",
        "workspace.dependencies",
    ];
    type Key<'a> = CargoSlot<'a>;

    fn locate(layout: &Layout, key: Self::Key<'_>) -> TomlFile<Self> {
        TomlFile::at(match key {
            CargoSlot::Workspace => layout.workspace_manifest(),
            CargoSlot::Unit(name) => layout.unit_crate_manifest(name),
            CargoSlot::System => layout.system_manifest(),
        })
    }
}

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct Workspace {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolver: Option<String>,
    /// `[workspace.package]` — fields members inherit with `workspace = true`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub package: Option<WorkspacePackage>,
    #[serde(default)]
    pub members: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exclude: Vec<String>,
    #[serde(default, skip_serializing_if = "Dependencies::is_empty")]
    pub dependencies: Dependencies,
    /// `workspace.lints` and anything else we do not model, kept verbatim.
    #[serde(flatten)]
    pub rest: Table<toml::Value>,
}

impl Workspace {
    /// The member directories this workspace declares, relative to `root`,
    /// with `exclude` applied. Sorted and deduplicated.
    pub fn member_dirs(
        &self,
        root: &std::path::Path,
    ) -> Result<Vec<std::path::PathBuf>, PatternError> {
        let mut excluded = Vec::new();
        for pattern in &self.exclude {
            excluded.extend(PathPattern::new(pattern).expand(root)?);
        }

        let mut dirs = Vec::new();
        for pattern in &self.members {
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
