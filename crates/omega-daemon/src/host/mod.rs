//! What a host does and a unit does not: watching the filesystem, staging
//! a directory before it replaces a live one, expanding globs, and finding
//! the units in a workspace. A plugin that draws a battery compiles none
//! of it.

pub mod cargo;
pub mod fs;
pub mod glob;
pub mod units;

pub use cargo::{CargoConfig, CargoManifest, CargoSlot};
pub use fs::{Changes, Recursion, StageDir, WatchError};
pub use glob::PatternError;
pub use units::{Units, UnitsError};
