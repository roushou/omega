//! Cargo build profile settings.
use crate::Table;
use serde::{Deserialize, Serialize};

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
