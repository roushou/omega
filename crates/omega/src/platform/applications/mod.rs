//! Installed application catalogue and typed activation.

mod activation;
mod catalogue;

pub use activation::Launcher;
pub use catalogue::{Application, ApplicationId, ApplicationIdError, Applications};
