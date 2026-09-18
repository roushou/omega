//! The Omega daemon: trust boundary, state owner, plugin supervisor.

pub mod action;
pub(crate) mod attachment;
pub mod authorization;
pub mod broker;
pub mod daemon;
pub mod events;
pub mod hub;
pub mod manifest;
pub mod plugins;
mod presentations;
pub mod process;
pub mod reconcile;
pub mod refusal;
pub mod schedule;
pub mod session;
pub mod shell;
pub mod shutdown;
pub mod state;
pub mod supervisor;
pub mod watch;

pub use authorization::Role;
pub use daemon::{Daemon, DaemonBuilder, DaemonError, DaemonHandle};
pub use manifest::ManifestStoreError;
pub use plugins::{Lifecycle, PluginRegistry, PluginToken, Transition};
pub use refusal::{Refusable, RefusableResult};
pub use session::{Liveness, Session, SessionError};
pub use shell::ShellError;
pub use shutdown::Shutdown;

pub mod storage;
