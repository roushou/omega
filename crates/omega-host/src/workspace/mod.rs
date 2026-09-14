//! Config source roles, Cargo documents, membership, and plugin discovery.

pub mod cargo;
mod pattern;
mod plugins;
mod role;
pub use pattern::{PathPattern, PatternError};
pub use plugins::{Plugins, PluginsError};
pub use role::{WorkspaceError, WorkspaceRole};
