//! `Cargo.toml`: the sections omega reads or writes, plus the data shapes a
//! generator declares its dependencies with.
//!
//! One model serves both directions — reading a user's workspace back out,
//! and emitting a generated one. Keys omega does not model round-trip
//! through [`CargoManifest::rest`] rather than being dropped on write.

use serde::{Deserialize, Serialize};

use crate::host::error::PatternError;
use crate::host::glob::PathPattern;
use omega_proto::Layout;
use omega_proto::TomlSchema;
use omega_proto::UnitName;
use omega_proto::toml::{Table, TomlFile};

/// A TOML table of Cargo dependencies, keyed by crate name.
pub type Dependencies = Table<Dependency>;

/// Which `Cargo.toml` in the config workspace.
#[derive(Debug, Clone, Copy)]
pub enum CargoSlot<'a> {
    /// `~/.config/omega/Cargo.toml`.
    Workspace,
    /// `~/.config/omega/units/<name>/Cargo.toml`.
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

/// `.cargo/config.toml` — cargo's own configuration for a directory tree.
///
/// One thing lives here and it is deliberate: `[patch]`, the override that
/// points a dependency at a checkout. A manifest is committed and has to
/// resolve on every machine that clones it, so it names published crates; a
/// path into somebody's home directory belongs in a file that stays on that
/// machine.
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct CargoConfig {
    /// Source name (`crates-io`) to the crates replaced within it.
    #[serde(default, skip_serializing_if = "Table::is_empty")]
    pub patch: Table<Dependencies>,
    /// Everything cargo understands and omega does not.
    #[serde(flatten)]
    pub rest: Table<toml::Value>,
}

impl CargoConfig {
    /// The source name a crates.io dependency is patched under.
    pub const REGISTRY: &'static str = "crates-io";

    /// The crates currently replaced by a local path.
    pub fn patched(&self) -> Option<&Dependencies> {
        self.patch.get(Self::REGISTRY)
    }

    /// Replace the whole patch table: linking is a statement about where the
    /// crates come from, not an addition to previous statements.
    pub fn replace_patch(&mut self, patched: Dependencies) {
        match patched.is_empty() {
            true => {
                self.patch.remove(Self::REGISTRY);
            }
            false => {
                self.patch.insert(Self::REGISTRY, patched);
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        self.patch.is_empty() && self.rest.is_empty()
    }
}

impl TomlSchema for CargoConfig {
    const KIND: &'static str = "cargo config";
    type Key<'a> = ();

    fn locate(layout: &Layout, _key: Self::Key<'_>) -> TomlFile<Self> {
        TomlFile::at(layout.cargo_config())
    }
}

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct Workspace {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolver: Option<String>,
    /// `[workspace.package]` — what members inherit with
    /// `version.workspace = true`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub package: Option<WorkspacePackage>,
    #[serde(default)]
    pub members: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exclude: Vec<String>,
    #[serde(default, skip_serializing_if = "Dependencies::is_empty")]
    pub dependencies: Dependencies,
    /// `workspace.package`, `workspace.lints`, and anything else we do not
    /// model, kept verbatim.
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

/// The fields a workspace's members inherit with `version.workspace = true`.
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkspacePackage {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(flatten)]
    pub rest: Table<toml::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Package {
    pub name: String,
    pub version: String,
    pub edition: String,
    #[serde(flatten)]
    pub rest: Table<toml::Value>,
}

impl Package {
    pub fn new(
        name: impl Into<String>,
        version: impl Into<String>,
        edition: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            version: version.into(),
            edition: edition.into(),
            rest: Table::new(),
        }
    }
}

/// A dependency entry, in either shape Cargo accepts: the bare version
/// string (`serde = "1"`) or a table (`serde = { version = "1", ... }`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Dependency {
    Version(String),
    Detailed(DependencyDetail),
}

impl Dependency {
    /// `{ workspace = true }` — inherits from the workspace manifest.
    pub fn inherited() -> Self {
        Self::Detailed(DependencyDetail {
            workspace: Some(true),
            ..Default::default()
        })
    }

    /// A registry requirement: the bare string when there are no features,
    /// the table shape when there are.
    pub fn registry(version: impl Into<String>, features: &[&str]) -> Self {
        match DependencyDetail::features(features) {
            None => Self::Version(version.into()),
            features => Self::Detailed(DependencyDetail {
                version: Some(version.into()),
                features,
                ..Default::default()
            }),
        }
    }

    /// `{ path = "..." }` — a local path dependency.
    pub fn local(path: impl Into<String>, features: &[&str]) -> Self {
        Self::Detailed(DependencyDetail {
            path: Some(path.into()),
            features: DependencyDetail::features(features),
            ..Default::default()
        })
    }

    /// `{ git = "...", tag = "..." }` — a dependency that resolves on any
    /// machine, which a path into somebody's checkout does not.
    pub fn git(url: impl Into<String>, tag: impl Into<String>, features: &[&str]) -> Self {
        Self::Detailed(DependencyDetail {
            git: Some(url.into()),
            tag: Some(tag.into()),
            features: DependencyDetail::features(features),
            ..Default::default()
        })
    }

    pub fn version(&self) -> Option<&str> {
        match self {
            Self::Version(v) => Some(v),
            Self::Detailed(d) => d.version.as_deref(),
        }
    }

    pub fn path(&self) -> Option<&str> {
        match self {
            Self::Version(_) => None,
            Self::Detailed(d) => d.path.as_deref(),
        }
    }

    pub fn repository(&self) -> Option<&str> {
        match self {
            Self::Version(_) => None,
            Self::Detailed(d) => d.git.as_deref(),
        }
    }

    pub fn is_inherited(&self) -> bool {
        matches!(self, Self::Detailed(d) if d.workspace == Some(true))
    }
}

/// The table shape of a dependency entry.
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct DependencyDetail {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub features: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace: Option<bool>,
    /// `git`, `optional`, `default-features`, … kept verbatim.
    #[serde(flatten)]
    pub rest: Table<toml::Value>,
}

impl DependencyDetail {
    fn features(features: &[&str]) -> Option<Vec<String>> {
        (!features.is_empty()).then(|| features.iter().map(|f| (*f).to_string()).collect())
    }
}

/// Where a generated dependency comes from. The declaration; [`Dependency`]
/// is the resolved result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DependencySource {
    /// A crates.io requirement.
    Registry(&'static str),
    /// One of omega's own crates, path-resolved against the source tree.
    OmegaCrate,
}

/// One line of a generator's dependency table.
#[derive(Debug, Clone, Copy)]
pub struct DependencySpec {
    pub name: &'static str,
    pub source: DependencySource,
    pub features: &'static [&'static str],
}

impl DependencySpec {
    pub const fn registry(name: &'static str, version: &'static str) -> Self {
        Self {
            name,
            source: DependencySource::Registry(version),
            features: &[],
        }
    }

    pub const fn omega(name: &'static str) -> Self {
        Self {
            name,
            source: DependencySource::OmegaCrate,
            features: &[],
        }
    }

    pub const fn with_features(mut self, features: &'static [&'static str]) -> Self {
        self.features = features;
        self
    }

    /// The `{ workspace = true }` entry a member crate uses to inherit this
    /// dependency. Derived, so a member's list can never drift from the
    /// workspace's.
    pub fn inherited(&self) -> (&'static str, Dependency) {
        (self.name, Dependency::inherited())
    }
}

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct Profile {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub release: Option<ReleaseProfile>,
    #[serde(flatten)]
    pub rest: Table<toml::Value>,
}

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReleaseProfile {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strip: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lto: Option<String>,
    #[serde(flatten)]
    pub rest: Table<toml::Value>,
}
