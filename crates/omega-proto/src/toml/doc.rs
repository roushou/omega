use std::path::Path;

use crate::error::TomlError;
use crate::toml::file::TomlFile;
use crate::toml::schema::TomlSchema;

/// A loaded document and the file it came from.
///
/// The difference from [`TomlFile`] is *time*. A `TomlFile` is an address:
/// every operation on it opens the file, does one thing, and closes it —
/// including `edit`, which reads, mutates and writes back in one call. A
/// `TomlDoc` is a value that has been loaded and remembers where it came
/// from, so it can be passed around, inspected, and changed over many steps
/// before anything is written.
///
/// Reach for a `TomlFile` when the whole change fits in one place, which is
/// almost always. Reach for a `TomlDoc` when the value has to outlive the
/// call that loaded it: a scaffold that builds a manifest across several
/// decisions and saves once at the end, an interactive edit, or anything that
/// must not re-read the file between steps because the first read is the one
/// being reasoned about.
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
