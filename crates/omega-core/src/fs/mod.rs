//! Filesystem primitives, all atomic.
//!
//! The daemon runs out of the state dir; a build that writes into it
//! incrementally can leave a torn state that a running daemon reads. These
//! types make the "last good state keeps running" promise true: write to a
//! sibling, then rename — which is atomic on the same filesystem.

mod atomic;
mod stage;
mod temp;
mod watch;

pub use atomic::AtomicFile;
pub use stage::StageDir;
pub use watch::{Changes, Recursion, WatchError};

pub(crate) use temp::TempPath;
