//! The Omega daemon: trust boundary, state owner, unit supervisor.

pub mod action;
pub mod daemon;
pub mod error;
pub mod events;
pub mod hub;
pub mod manifest;
pub mod process;
pub mod reconcile;
pub mod refusal;
pub mod session;
pub mod shell;
pub mod shutdown;
pub mod source;
pub mod sources;
pub mod state;
pub mod supervisor;
pub mod units;
pub mod watch;

pub use daemon::{Daemon, DaemonBuilder, DaemonHandle};
pub use error::{DaemonError, ManifestStoreError, SessionError, ShellError};
pub use refusal::{Refusable, RefusableResult};
pub use session::{Liveness, Role, Session};
pub use shutdown::Shutdown;
pub use units::{Lifecycle, Transition, UnitTable, UnitToken};
