//! The shell-sourceable session environment.

use std::collections::BTreeMap;
use std::path::PathBuf;

use omega_host::{AtomicFile, Layout};
use omega_proto::omega::StateDocument;

use crate::reconcile::ProviderError;

#[derive(Debug)]
pub struct EnvironmentProvider {
    path: PathBuf,
}

impl EnvironmentProvider {
    pub fn new(layout: &Layout) -> Self {
        Self {
            path: layout.environment(),
        }
    }

    pub fn path(&self) -> &std::path::Path {
        &self.path
    }

    fn desired(document: &StateDocument) -> BTreeMap<String, String> {
        document
            .environment
            .iter()
            .map(|variable| (variable.key.clone(), variable.value.clone()))
            .collect()
    }

    fn render(variables: &BTreeMap<String, String>) -> String {
        variables
            .iter()
            .map(|(key, value)| {
                let value = if value
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"_./:-@,".contains(&byte))
                {
                    value.clone()
                } else {
                    format!("'{}'", value.replace('\'', "'\\''"))
                };
                format!("{key}={value}\n")
            })
            .collect()
    }

    /// Validate environment declarations and retain their shell-sourceable contents.
    pub fn prepare(document: &StateDocument) -> Result<EnvironmentChange, ProviderError> {
        omega_document::DocumentValidation::environment(document)
            .map_err(|error| ProviderError::new("environment", error.to_string()))?;
        Ok(EnvironmentChange {
            contents: Self::render(&Self::desired(document)),
        })
    }

    /// Read installed bytes. Only a missing file is absence; other read errors propagate.
    pub fn installed(&self) -> Result<Option<Vec<u8>>, ProviderError> {
        match std::fs::read(&self.path) {
            Ok(actual) => Ok(Some(actual)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(ProviderError::new("environment", error.to_string())),
        }
    }

    /// Compare prepared contents with captured bytes. Absence satisfies empty contents.
    pub fn plan(desired: EnvironmentChange, installed: Option<&[u8]>) -> Option<EnvironmentChange> {
        (installed.unwrap_or_default() != desired.contents.as_bytes()).then_some(desired)
    }

    pub fn apply(&self, change: &EnvironmentChange) -> Result<(), ProviderError> {
        tracing::info!("publishing session environment");
        AtomicFile::at(&self.path)
            .write(change.contents.as_bytes())
            .map_err(|error| ProviderError::new("environment", error.to_string()))
    }
}

/// Validated, rendered contents; applying never reinterprets the document.
#[derive(Debug)]
pub struct EnvironmentChange {
    contents: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use omega_document::Document;

    #[test]
    fn comparison_preserves_absence_empty_files_and_arbitrary_bytes() {
        for (installed, expected) in [
            (None, false),
            (Some(b"".as_slice()), false),
            (Some(b"EDITOR=hx\n".as_slice()), true),
            (Some(b"\xff".as_slice()), true),
        ] {
            let desired = EnvironmentProvider::prepare(&StateDocument::default()).unwrap();
            let change = EnvironmentProvider::plan(desired, installed);
            assert_eq!(change.is_some(), expected);
            if let Some(change) = change {
                assert_eq!(change.contents, "");
            }
        }

        let document = Document::new().env("EDITOR", "hx").into_inner();
        for (installed, expected) in [
            (None, true),
            (Some(b"".as_slice()), true),
            (Some(b"EDITOR=hx\n".as_slice()), false),
            (Some(b"EDITOR=hx".as_slice()), true),
            (Some(b"\xff".as_slice()), true),
        ] {
            let desired = EnvironmentProvider::prepare(&document).unwrap();
            let change = EnvironmentProvider::plan(desired, installed);
            assert_eq!(change.is_some(), expected);
            if let Some(change) = change {
                assert_eq!(change.contents, "EDITOR=hx\n");
            }
        }
    }
}
