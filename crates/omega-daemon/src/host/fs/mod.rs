//! The filesystem work only a host does: watching for changes.
//!
//! Staging moved to `omega-proto` — a directory swapped in by rename costs
//! nothing to carry, and the renderer installs the same way a build stages.
//! Re-exported here so a host reaches for one filesystem vocabulary.

mod watch;

pub use omega_proto::{AtomicFile, StageDir, TempPath};
pub use watch::{Changes, Recursion, WatchError};
