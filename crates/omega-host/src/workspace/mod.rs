//! Omega config source roles and runnable plugin discovery.

mod plugins;
mod role;
pub use plugins::{Plugins, PluginsError};
pub use role::{WorkspaceError, WorkspaceRole};
