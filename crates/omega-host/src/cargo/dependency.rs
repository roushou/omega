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
    /// Additional Cargo options, such as `optional` and `default-features`.
    #[serde(flatten)]
    pub rest: Table<toml::Value>,
}

impl DependencyDetail {
    fn features(features: &[&str]) -> Option<Vec<String>> {
        (!features.is_empty()).then(|| features.iter().map(|f| (*f).to_string()).collect())
    }
}

/// Conversion at the source-document boundary; callers never reserialize the whole manifest.
pub(super) struct DependencyTable;

impl DependencyTable {
    pub(super) fn read(
        table: Option<&dyn toml_edit::TableLike>,
        path: &str,
    ) -> Result<Dependencies, super::CargoError> {
        use serde::de::IntoDeserializer;
        let mut result = Dependencies::new();
        if let Some(table) = table {
            for (name, item) in table.iter() {
                let value = item.clone().into_value().map_err(|_| {
                    super::CargoError::new(format!("{path}.{name}"), "invalid dependency")
                })?;
                let dependency =
                    Dependency::deserialize(value.into_deserializer()).map_err(|error| {
                        super::CargoError::new(format!("{path}.{name}"), error.to_string())
                    })?;
                result.insert(name, dependency);
            }
        }
        Ok(result)
    }

    pub(super) fn item(dependency: &Dependency) -> Result<toml_edit::Item, super::CargoError> {
        let mut value = dependency
            .serialize(toml_edit::ser::ValueSerializer::new())
            .map_err(|error| super::CargoError::new("dependency", error.to_string()))?;
        if let Some(table) = value.as_inline_table_mut() {
            table.fmt();
        }
        Ok(toml_edit::Item::Value(value))
    }

    pub(super) fn table(
        dependencies: &Dependencies,
    ) -> Result<toml_edit::Table, super::CargoError> {
        let mut table = toml_edit::Table::new();
        for (name, dependency) in dependencies.iter() {
            table.insert(name, Self::item(dependency)?);
        }
        Ok(table)
    }
}
