//! Filesystem primitives that cost nothing to carry: a write that is
//! either wholly there or wholly absent, the temporary it goes through, and
//! the directory that replaces a live one in a single rename. The watching
//! half is a host concern and lives with the daemon.

mod atomic;
mod stage;
mod temp;

pub use atomic::AtomicFile;
pub use stage::StageDir;
pub use temp::TempPath;
