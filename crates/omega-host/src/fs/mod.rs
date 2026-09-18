//! Atomic file publication, durable directories and staged replacement.

mod atomic;
mod directory;
mod stage;
mod temp;

pub use atomic::{AtomicFile, WriteError};
pub use directory::Directory;
pub use stage::StageDir;
pub use temp::TempPath;

mod lock;
pub use lock::FileLock;

#[cfg(feature = "watch")]
mod watch;
#[cfg(feature = "watch")]
pub use watch::{Changes, Recursion, WatchError};
