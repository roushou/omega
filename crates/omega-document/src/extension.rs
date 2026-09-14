use crate::StateDocument;

/// A typed integration that contributes desired state to a document.
///
/// Implementations compute declarations without applying desktop effects.
/// Integration-specific payloads must be validated by their host adapter.
pub trait DocumentExtension {
    type Error: std::error::Error + Send + Sync + 'static;

    fn apply(self, document: &mut StateDocument) -> Result<(), Self::Error>;
}
