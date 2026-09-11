//! Converging the machine toward the state document.
//!
//! A reconciler is not a script. Each provider owns one domain, *plans* what
//! would change without touching anything, and only then applies it — so a
//! change can be shown before it happens, and applying nothing is the normal
//! outcome of a machine that already matches its document.

pub mod bars;
mod build;
pub mod converger;
pub mod environment;
pub mod schedules;
pub mod shell;
pub mod units;

pub use bars::BarProvider;
pub use build::ValidatedBuild;
pub use converger::{Context, Converger, Work};
pub use environment::EnvironmentProvider;
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
