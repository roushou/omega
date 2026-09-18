//! Command identities, contracts, dependencies, and terminal answers.

mod address;
mod answer;
mod catalogue;
mod dependency;
mod shape;
mod signature;

pub use address::CommandAddress;
pub use answer::CommandAnswer;
pub use dependency::CommandContracts;

#[derive(Debug, thiserror::Error)]
pub enum CommandContractError {
    #[error(transparent)]
    Identifier(#[from] crate::IdentError),
    #[error("invalid command contract: {0}")]
    Invalid(String),
}
