//! The filesystem work only a host does: staging a directory before it
//! replaces a live one, and watching for changes.

mod stage;
mod watch;

pub use omega_proto::{AtomicFile, TempPath};
pub use stage::StageDir;
pub use watch::{Changes, Recursion, WatchError};
