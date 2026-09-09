//! Session environment.
//!
//! The document declares variables; this writes the file a session sources.
//! It is the smallest possible second provider, and it exists to keep the
//! [`Provider`] trait honest: a trait with one implementation is a shape
//! nobody has tested.

use std::collections::BTreeMap;
use std::path::PathBuf;

use async_trait::async_trait;

use omega_host::{AtomicFile, Layout};
use omega_proto::omega::StateDocument;

use crate::reconcile::{Change, Provider, ProviderError};

#[derive(Debug)]
pub struct EnvironmentProvider {
    path: PathBuf,
}

impl EnvironmentProvider {
    /// `~/.local/state/omega/environment`, in the shell-sourceable shape.
    pub const FILE_NAME: &'static str = "environment";

    pub fn new(layout: &Layout) -> Self {
        Self {
            path: layout.state.join(Self::FILE_NAME),
        }
    }

    pub fn at(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
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

    /// What the file says now. An unreadable or absent file is "nothing set",
    /// which converges to the document rather than failing.
    fn actual(&self) -> BTreeMap<String, String> {
        let Ok(contents) = std::fs::read_to_string(&self.path) else {
            return BTreeMap::new();
        };

        contents
            .lines()
            .filter_map(|line| line.split_once('='))
            .map(|(key, value)| (key.trim().to_string(), value.trim().to_string()))
            .collect()
    }

    fn render(variables: &BTreeMap<String, String>) -> String {
        variables
            .iter()
            .map(|(key, value)| format!("{key}={value}\n"))
            .collect()
    }
}

#[async_trait]
impl Provider for EnvironmentProvider {
    fn domain(&self) -> &'static str {
        "environment"
    }

    fn plan(&self, document: &StateDocument) -> Vec<Change> {
        let desired = Self::desired(document);
        let actual = self.actual();

        let mut changes: Vec<Change> = desired
            .iter()
            .filter_map(|(key, value)| match actual.get(key) {
                None => Some(Change::create(key, format!("{key}={value}"))),
                Some(current) if current != value => {
                    Some(Change::update(key, format!("{current} -> {value}")))
                }
                Some(_) => None,
            })
            .chain(
                actual
                    .keys()
                    .filter(|key| !desired.contains_key(*key))
                    .map(|key| Change::delete(key, "no longer declared")),
            )
            .collect();

        changes.sort_by(|a, b| a.target.cmp(&b.target));
        changes
    }

    /// The file is rewritten whole from the document: it is a projection of
    /// the document, and a projection is never patched in place.
    async fn apply(
        &self,
        document: &StateDocument,
        _changes: &[Change],
    ) -> Result<(), ProviderError> {
        AtomicFile::at(&self.path)
            .write(Self::render(&Self::desired(document)).as_bytes())
            .map_err(|e| ProviderError::new("environment", e.to_string()))
    }
}
