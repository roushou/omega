//! Validated identifiers for units, surfaces, modules, and instances.
//! Parse identifiers at input boundaries and retain their typed values internally.

use std::fmt;

use serde::{Deserialize, Serialize};

/// Shared identifier validation; public newtypes select their identifier kind.
pub(crate) struct Ident;

impl Ident {
    pub(crate) fn parse(kind: &'static str, name: String) -> Result<String, IdentError> {
        if name.is_empty()
            || !name
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
        {
            return Err(IdentError::InvalidCharacters { kind, name });
        }
        if !name.starts_with(|c: char| c.is_ascii_lowercase()) {
            return Err(IdentError::InvalidStart { kind, name });
        }
        Ok(name)
    }
}

/// Unit identifier using lowercase ASCII letters, digits, hyphens, and underscores.
/// Must start with a letter. Serializes as a string.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub struct UnitName(String);

impl UnitName {
    /// Validate and wrap a unit name.
    pub fn parse(name: impl Into<String>) -> Result<Self, IdentError> {
        Ident::parse("unit name", name.into()).map(Self)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for UnitName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Surface identifier, unique within its declaring unit.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub struct SurfaceId(String);

impl SurfaceId {
    pub fn parse(id: impl Into<String>) -> Result<Self, IdentError> {
        Ident::parse("surface id", id.into()).map(Self)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SurfaceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A bar module's id: one instance of a surface, named by the state document
/// so the same widget can appear twice.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub struct ModuleId(String);

impl ModuleId {
    pub fn parse(id: impl Into<String>) -> Result<Self, IdentError> {
        Ident::parse("module id", id.into()).map(Self)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ModuleId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Invalid identifier with its kind and validation failure.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum IdentError {
    #[error("{kind} exceeds 128 bytes")]
    TooLong { kind: &'static str },
    #[error("{kind} must be lowercase letters, digits, hyphens and underscores: {name:?}")]
    InvalidCharacters { kind: &'static str, name: String },
    #[error("{kind} must start with a lowercase letter: {name:?}")]
    InvalidStart { kind: &'static str, name: String },
}

impl<'de> Deserialize<'de> for UnitName {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::parse(String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

impl<'de> Deserialize<'de> for SurfaceId {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::parse(String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

impl<'de> Deserialize<'de> for ModuleId {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::parse(String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}
