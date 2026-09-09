//! The host side: where things live on disk, and how they get written there.
//!
//! None of this crosses a socket, which is why it is not in `omega-proto`.
//! A unit talks to the daemon and never reads the config workspace, stages a
//! directory, or parses a TOML document — so a plugin that draws a battery
//! compiles none of it.
//!
//! Three things, and they only ever appear together: [`Layout`] says where a
//! file belongs, [`TomlFile`] says what shape it has, and [`AtomicFile`]
//! says a reader never sees half of one.

pub mod fs;
pub mod layout;
pub mod toml;

pub use fs::{AtomicFile, StageDir, TempPath};
pub use layout::{Layout, Profile};
pub use toml::{Table, Toml, TomlDoc, TomlError, TomlFile, TomlSchema};
