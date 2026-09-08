//! The Omega daemon: trust boundary, state owner, unit supervisor.

pub mod action;
pub mod broker;
pub mod daemon;
pub mod events;
pub mod host;
pub mod hub;
pub mod manifest;
pub mod process;
pub mod reconcile;
pub mod refusal;
pub mod session;
pub mod shell;
pub mod shutdown;
pub mod state;
pub mod supervisor;
pub mod units;
pub mod watch;

pub use daemon::{Daemon, DaemonBuilder, DaemonError, DaemonHandle};
pub use manifest::ManifestStoreError;
pub use refusal::{Refusable, RefusableResult};
pub use session::{Liveness, Role, Session, SessionError};
pub use shell::ShellError;
pub use shutdown::Shutdown;
pub use units::{Lifecycle, Transition, UnitTable, UnitToken};
