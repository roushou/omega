use crate::UnitName;

/// A manifest that parsed but does not describe a unit the daemon can trust.
/// Fail loud: a typo'd capability or surface kind must be a build error,
/// never a silently dropped grant.
#[derive(Debug, thiserror::Error)]
pub enum ManifestError {
    #[error("unknown capability {0:?}")]
    UnknownCapability(String),
    #[error("unknown surface kind {0:?}")]
    UnknownSurfaceKind(String),
    #[error("{0}")]
    UnknownStateTopic(#[from] crate::TopicError),
    #[error("unknown event {0:?}")]
    UnknownEvent(String),
    #[error("unit {expected}: manifest declares name {declared:?} — they must match")]
    NameMismatch {
        expected: UnitName,
        declared: UnitName,
    },
}
