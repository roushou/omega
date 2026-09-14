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

impl InstanceKey {
    pub fn parse(value: &crate::omega::InstanceRef) -> Result<Self, crate::IdentError> {
        Ok(Self {
            id: InstanceId::parse(&value.id)?,
            incarnation: IncarnationId::parse(&value.incarnation)?,
        })
    }

    pub fn wire(&self) -> crate::omega::InstanceRef {
        crate::omega::InstanceRef {
            id: self.id.to_string(),
            incarnation: self.incarnation.to_string(),
        }
    }
}
