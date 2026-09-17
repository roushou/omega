//! Filesystem paths, atomic writes, build generations, and source workspace metadata.
//! [`Layout`] resolves paths, [`TomlFile`] accesses typed documents, and
//! [`AtomicFile`] publishes file contents atomically.

pub mod cargo;
pub mod fs;
pub mod generation;
pub mod layout;
pub mod package;
pub mod process;
pub mod recovery;
pub mod state;
pub mod systemd;
pub mod toml;
pub mod workspace;

pub use fs::{AtomicFile, Directory, StageDir, TempPath};
pub use generation::{Generation, GenerationId, GenerationStage, Generations, Rollback};
pub use layout::{Layout, Profile};
pub use state::{BuiltPlugin, StateConfig};
pub use toml::{Table, Toml, TomlDoc, TomlError, TomlFile, TomlSchema};
