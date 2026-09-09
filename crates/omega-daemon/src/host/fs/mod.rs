//! The filesystem work only this daemon does: watching for changes.
//!
//! Staging lives in `omega-host`, because the CLI stages a build the same
//! way the renderer installs a shell. Re-exported here so a host reaches for
//! one filesystem vocabulary rather than two.

mod watch;

pub use omega_host::{AtomicFile, StageDir, TempPath};
pub use watch::{Changes, Recursion, WatchError};
