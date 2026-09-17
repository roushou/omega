//! Validated runtime identities and presentation declarations.
mod identity;
mod presentation;

pub use identity::{IncarnationId, InstanceId, PlacementId, SingletonId};
pub use presentation::{PresentationError, PresentationSpec};

/// Both fields are required on every instance-scoped operation.
#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
pub struct InstanceKey {
    pub id: InstanceId,
    pub incarnation: IncarnationId,
}

impl TryFrom<&crate::omega::InstanceRef> for InstanceKey {
    type Error = crate::IdentError;

    fn try_from(value: &crate::omega::InstanceRef) -> Result<Self, Self::Error> {
        Ok(Self {
            id: value.id.parse::<InstanceId>()?,
            incarnation: value.incarnation.parse::<IncarnationId>()?,
        })
    }
}

impl InstanceKey {
    pub fn wire(&self) -> crate::omega::InstanceRef {
        crate::omega::InstanceRef {
            id: self.id.to_string(),
            incarnation: self.incarnation.to_string(),
        }
    }
}

mod fingerprint;
pub use fingerprint::{RendererFingerprint, RendererFingerprintError};
