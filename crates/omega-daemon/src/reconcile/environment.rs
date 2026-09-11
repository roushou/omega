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
    pub fn plan(
        &self,
        document: &StateDocument,
    ) -> Result<Option<EnvironmentChange>, ProviderError> {
        omega_document::DocumentValidation::environment(document)
            .map_err(|error| ProviderError::new("environment", error.to_string()))?;
        let contents = Self::render(&Self::desired(document));
        match std::fs::read(&self.path) {
            Ok(actual) if actual == contents.as_bytes() => Ok(None),
            Ok(_) => Ok(Some(EnvironmentChange { contents })),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                Ok((!contents.is_empty()).then_some(EnvironmentChange { contents }))
            }
            Err(error) => Err(ProviderError::new("environment", error.to_string())),
        }
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
