use std::marker::PhantomData;
use std::path::{Path, PathBuf};

use super::{ReadError, TomlError};
use crate::fs::AtomicFile;
use crate::toml::schema::{TomlSchema, Validated};
use crate::toml::{Toml, TomlDoc};

/// A typed address: this path holds a document of schema `S`.
///
/// Constructing one does no I/O, so a file can be located, passed around, and
/// stored long before it is touched.
pub struct TomlFile<S> {
    path: PathBuf,
    schema: PhantomData<fn() -> S>,
}

impl<S> Clone for TomlFile<S> {
    fn clone(&self) -> Self {
        Self {
            path: self.path.clone(),
            schema: PhantomData,
        }
    }
}

impl<S> std::fmt::Debug for TomlFile<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("TomlFile").field(&self.path).finish()
    }
}

impl<S: TomlSchema> TomlFile<S> {
    pub fn at(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            schema: PhantomData,
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn into_path(self) -> PathBuf {
        self.path
    }

    pub fn exists(&self) -> bool {
        self.path.exists()
    }

    pub fn read(&self) -> Result<S, TomlError> {
        let src = std::fs::read_to_string(&self.path).map_err(|source| TomlError::Read {
            kind: S::KIND,
            path: self.path.clone(),
            source,
        })?;
        toml::from_str(&src).map_err(|source| TomlError::Parse {
            kind: S::KIND,
            path: self.path.clone(),
            source: Box::new(source),
        })
    }

    /// Read the document, or its default when the file does not exist. Any
    /// other read failure is still an error.
    pub fn read_or_default(&self) -> Result<S, TomlError>
    where
        S: Default,
    {
        match self.read() {
            Err(e) if e.is_not_found() => Ok(S::default()),
            other => other,
        }
    }

    /// Serialize and write atomically, creating parent directories.
    pub fn write(&self, value: &S) -> Result<(), TomlError> {
        let bytes = Toml::encode(value)?;
        AtomicFile::at(&self.path)
            .write(bytes.as_bytes())
            .map_err(|source| TomlError::Write {
                kind: S::KIND,
                path: self.path.clone(),
                source,
            })
    }

    /// Write only when the file is absent. Returns whether it was written.
    pub fn create_new(&self, value: &S) -> Result<bool, TomlError> {
        if self.exists() {
            return Ok(false);
        }
        self.write(value)?;
        Ok(true)
    }

    /// Read, mutate, write back atomically.
    pub fn edit<R>(&self, f: impl FnOnce(&mut S) -> R) -> Result<R, TomlError> {
        let mut value = self.read()?;
        let out = f(&mut value);
        self.write(&value)?;
        Ok(out)
    }

    /// Load the document as a [`TomlDoc`], which remembers this path.
    pub fn open(&self) -> Result<TomlDoc<S>, TomlError> {
        Ok(TomlDoc::new(self.read()?, self.clone()))
    }
}

impl<S: Validated> TomlFile<S> {
    /// Read, then check the schema's invariants against `cx`.
    pub fn read_valid(&self, cx: S::Context<'_>) -> Result<S, ReadError<S::Invalid>> {
        let value = self.read()?;
        value.validate(cx).map_err(ReadError::Invalid)?;
        Ok(value)
    }
}
