//! Validated numbered and named workspace destinations.

use crate::{FromValue, IntoValue, omega::Value};
use std::fmt;

/// A numbered workspace in the range 1 through 2,147,483,647.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WorkspaceIndex(u32);

impl WorkspaceIndex {
    pub fn new(index: u32) -> Result<Self, WorkspaceIndexError> {
        if index == 0 || index > i32::MAX as u32 {
            return Err(WorkspaceIndexError(index));
        }
        Ok(Self(index))
    }

    pub fn get(self) -> u32 {
        self.0
    }
}

impl fmt::Display for WorkspaceIndex {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("workspace index must be between 1 and 2147483647: {0}")]
pub struct WorkspaceIndexError(u32);

impl FromValue for WorkspaceIndex {
    fn from_value(value: &Value) -> Option<Self> {
        Self::new(u32::from_value(value)?).ok()
    }
}

impl IntoValue for WorkspaceIndex {
    fn into_value(self) -> Value {
        self.0.into_value()
    }
}

/// A literal workspace name, without compositor selector prefixes.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WorkspaceName(String);

impl WorkspaceName {
    /// Accept nonblank names without control characters. Preserve spaces and case.
    pub fn parse(name: impl Into<String>) -> Result<Self, WorkspaceNameError> {
        let name = name.into();
        if name.trim().is_empty() || name.chars().any(char::is_control) {
            return Err(WorkspaceNameError(name));
        }
        Ok(Self(name))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for WorkspaceName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("workspace name must be nonblank and contain no control characters: {0:?}")]
pub struct WorkspaceNameError(String);

impl FromValue for WorkspaceName {
    fn from_value(value: &Value) -> Option<Self> {
        Self::parse(String::from_value(value)?).ok()
    }
}

impl IntoValue for WorkspaceName {
    fn into_value(self) -> Value {
        self.0.into_value()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indices_validate_and_round_trip_without_truncation() {
        for index in [1, 10, i32::MAX as u32] {
            let index = WorkspaceIndex::new(index).unwrap();
            assert_eq!(WorkspaceIndex::from_value(&index.into_value()), Some(index));
        }
        for index in [0, i32::MAX as u32 + 1, u32::MAX] {
            assert!(WorkspaceIndex::new(index).is_err());
            assert!(WorkspaceIndex::from_value(&index.into_value()).is_none());
        }
        for value in [
            (-1_i64).into_value(),
            1.5_f64.into_value(),
            true.into_value(),
        ] {
            assert!(WorkspaceIndex::from_value(&value).is_none());
        }
    }

    #[test]
    fn names_are_literal_and_round_trip() {
        for name in ["mail", "2", "next", "仕事", "Work notes", "a\"b\\c;d,e"] {
            let name = WorkspaceName::parse(name).unwrap();
            assert_eq!(
                WorkspaceName::from_value(&name.clone().into_value()),
                Some(name)
            );
        }
        for name in ["", " ", "a\nb", "a\rb", "a\0b", "a\tb"] {
            assert!(WorkspaceName::parse(name).is_err());
        }
    }
}
