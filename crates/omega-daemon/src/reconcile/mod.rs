//! Plan and apply desired-state changes per domain.
//! Planning is pure; applying an empty plan leaves runtime state unchanged.

mod build;
pub mod converger;
pub mod deployment;
pub mod environment;
pub mod presentations;
pub mod schedules;
pub mod shell;
pub mod units;

pub use build::ValidatedBuild;
pub use converger::{Context, Converger, Work};
pub use environment::EnvironmentProvider;
pub use presentations::PresentationProvider;
pub use schedules::ScheduleProvider;
pub use units::UnitProvider;

/// What a change does to an entity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Create,
    Update,
    Delete,
}

#[derive(Debug, thiserror::Error)]
#[error("{domain}: {message}")]
pub struct ProviderError {
    pub domain: &'static str,
    pub message: String,
}

impl ProviderError {
    pub fn new(domain: &'static str, message: impl Into<String>) -> Self {
        Self {
            domain,
            message: message.into(),
        }
    }
}
