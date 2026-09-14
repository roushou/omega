use omega_document::{StateDocument, ValidationError};
use omega_proto::Manifest;

/// Validates Omarchy declarations and their projected widget instances together.
#[derive(Debug)]
pub struct DocumentValidation;

impl DocumentValidation {
    pub fn validate<'a>(
        document: &StateDocument,
        manifests: impl IntoIterator<Item = &'a Manifest>,
    ) -> Result<(), ValidationError> {
        let compiled = crate::shell::CompiledShell::of(document)
            .map_err(|error| ValidationError::Cause(Box::new(error)))?;
        let mut projected = document.clone();
        if let Some(shell) = compiled {
            projected.bars.push(shell.bar().clone());
        }
        projected.shell_json.clear();
        omega_document::DocumentValidation::validate(&projected, manifests)
    }
}
