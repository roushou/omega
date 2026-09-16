use serde::{Serialize, de::DeserializeOwned};

/// Facts observed by the change, including after an interrupted write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Observation {
    Before,
    After,
    /// The current state matches both snapshots; no replacement is needed.
    Unchanged,
    Conflict,
}

/// A serializable recovery contract. `KIND` versions the stored payload.
/// Implementations must validate deserialized data before effects, and recheck
/// preconditions at their effect boundary. The store serializes cooperating
/// writers; it cannot lock unrelated applications out of the target.
pub trait Change: Serialize + DeserializeOwned {
    const KIND: &'static str;
    type Error: std::error::Error + Send + Sync + 'static;

    fn inspect(&self) -> Result<Observation, Self::Error>;
    /// Return success only after the intended effects are durable.
    fn apply(&self) -> Result<(), Self::Error>;
    /// Restore the before-state and establish its durability before returning.
    fn restore(&self) -> Result<(), Self::Error>;
    /// Establish durability of an already observed before/after state without
    /// replaying the effect. Recheck the observed state before returning.
    /// Required when acknowledging an interrupted write.
    fn confirm(&self, observed: Observation) -> Result<(), Self::Error>;
}
