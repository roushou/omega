use std::path::Path;

use super::TomlError;
use crate::toml::file::TomlFile;
use crate::toml::schema::TomlSchema;

/// A loaded TOML value with its source path.
/// Use for edits spanning multiple steps followed by one save. [`TomlFile`]
/// performs individual reads, writes, or read-modify-write operations.
#[derive(Debug, Clone)]
pub struct TomlDoc<S> {
    value: S,
    file: TomlFile<S>,
}

impl<S: TomlSchema> TomlDoc<S> {
    pub fn new(value: S, file: TomlFile<S>) -> Self {
        Self { value, file }
    }

    pub fn value(&self) -> &S {
        &self.value
    }

    pub fn value_mut(&mut self) -> &mut S {
        &mut self.value
    }

    pub fn into_value(self) -> S {
        self.value
    }

    pub fn file(&self) -> &TomlFile<S> {
        &self.file
    }

    pub fn path(&self) -> &Path {
        self.file.path()
    }

    pub fn save(&self) -> Result<(), TomlError> {
        self.file.write(&self.value)
    }
}
