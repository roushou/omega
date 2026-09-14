//! Package identity and workspace inheritance.
use crate::Table;
use serde::{Deserialize, Serialize};

/// The package fields a workspace's members can inherit.
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkspacePackage {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edition: Option<String>,
    #[serde(flatten)]
    pub rest: Table<toml::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Package {
    pub name: String,
    pub version: String,
    pub edition: Edition,
    #[serde(flatten)]
    pub rest: Table<toml::Value>,
}

impl Package {
    pub fn new(name: impl Into<String>, version: impl Into<String>, edition: Edition) -> Self {
        Self {
            name: name.into(),
            version: version.into(),
            edition,
            rest: Table::new(),
        }
    }
}

/// Cargo accepts an explicit edition or inheritance from `[workspace.package]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Edition {
    Explicit(String),
    Inherited { workspace: bool },
}

impl Edition {
    pub fn inherited() -> Self {
        Self::Inherited { workspace: true }
    }
}
