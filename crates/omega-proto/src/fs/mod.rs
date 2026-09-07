//! Filesystem primitives that cost nothing to carry: a write that is
//! either wholly there or wholly absent, and the temporary it goes
//! through. The watching half is a host concern and lives with the daemon.

mod atomic;
mod temp;

pub use atomic::AtomicFile;
pub use temp::TempPath;
