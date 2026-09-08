//! The names things have.
//!
//! Every identifier omega passes around is parsed once at the edge and then
//! carried as a newtype, so an unvalidated string can never reach the
//! filesystem, the wire, or a map key. They share one rule — lowercase
//! letters, digits, hyphens and underscores, starting with a letter —
//! because a unit, a surface and a bar module are all names a person types
//! and later has to match by eye.

use std::fmt;

use serde::{Deserialize, Serialize};

/// The shared rule. Kept private: a caller names the *kind* of identifier it
/// wants, and the kind decides what is allowed.
struct Ident;

impl Ident {
    fn parse(kind: &'static str, name: String) -> Result<String, IdentError> {
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

/// A unit's identity: lowercase letters, digits, and hyphens, starting with a
/// letter (matching cargo's crate-name rules). Serializes transparently as a
/// plain string so `units.toml` keeps its current shape.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
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

/// A surface's id within its unit: what a unit calls one of the faces it
/// exposes. Unique only within the unit that declares it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
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

/// A name that does not obey the one rule every omega identifier follows.
/// The kind is carried so the message says what was being named.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum IdentError {
    #[error("{kind} must be lowercase letters, digits, hyphens and underscores: {name:?}")]
    InvalidCharacters { kind: &'static str, name: String },
    #[error("{kind} must start with a lowercase letter: {name:?}")]
    InvalidStart { kind: &'static str, name: String },
}
