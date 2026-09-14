//! Dependency declarations and scaffold specifications.
use crate::Table;
use serde::{Deserialize, Serialize};

/// Cargo dependencies keyed by package alias.
pub type Dependencies = Table<Dependency>;

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

    /// Registry dependency with a local alias, such as omega for package omega-rs.
    pub fn renamed(
        package: impl Into<String>,
        version: impl Into<String>,
        features: &[&str],
    ) -> Self {
        Self::Detailed(DependencyDetail {
            version: Some(version.into()),
            package: Some(package.into()),
            features: DependencyDetail::features(features),
            ..Default::default()
        })
    }

    /// `{ path = "..." }` — a local path dependency.
    pub fn local(path: impl Into<String>, features: &[&str]) -> Self {
        Self::Detailed(DependencyDetail {
            path: Some(path.into()),
            features: DependencyDetail::features(features),
            ..Default::default()
        })
    }

    /// Construct a Git dependency pinned to a tag.
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

    /// The registry name, when the table calls it something else.
    pub fn package(&self) -> Option<&str> {
        match self {
            Self::Version(_) => None,
            Self::Detailed(d) => d.package.as_deref(),
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
    /// The name on the registry, when it differs from the name in the table.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub package: Option<String>,
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
    /// What the registry calls it, when that is not `name`.
    pub package: Option<&'static str>,
    pub source: DependencySource,
    pub features: &'static [&'static str],
}

impl DependencySpec {
    pub const fn registry(name: &'static str, version: &'static str) -> Self {
        Self {
            name,
            package: None,
            source: DependencySource::Registry(version),
            features: &[],
        }
    }

    pub const fn omega(name: &'static str) -> Self {
        Self {
            name,
            package: None,
            source: DependencySource::OmegaCrate,
            features: &[],
        }
    }

    /// Called one thing in a manifest, published as another.
    pub const fn published_as(mut self, package: &'static str) -> Self {
        self.package = Some(package);
        self
    }

    /// What the registry — and so a `[patch]` table — calls this crate.
    pub const fn package(&self) -> &'static str {
        match self.package {
            Some(package) => package,
            None => self.name,
        }
    }

    pub const fn with_features(mut self, features: &'static [&'static str]) -> Self {
        self.features = features;
        self
    }

    /// Construct a workspace-inherited dependency entry.
    pub fn inherited(&self) -> (&'static str, Dependency) {
        (self.name, Dependency::inherited())
    }
}
