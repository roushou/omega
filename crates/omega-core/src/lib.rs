//! Omega's foundations: the on-disk contract, atomic filesystem primitives,
//! TOML documents declared as schemas, and validated names.
//!
//! Primitives only. The documents themselves live with the crates that mean
//! something by them — this one knows where a file goes, not what is in it.
//!
//! These are the primitives the CLI and daemon both build on. They exist so
//! the on-disk layout is declared exactly once, writes are atomic, names are
//! validated at the edge rather than trusted as strings, and every TOML
//! document has one declaration of what it is and where it lives.

pub mod error;
pub mod fs;
pub mod glob;
pub mod ident;
pub mod layout;
pub mod toml;
pub mod units;

pub use error::{IdentError, PatternError, ReadError, TomlError, UnitsError};
pub use fs::{AtomicFile, Changes, Recursion, StageDir, WatchError};
pub use ident::{ModuleId, SurfaceId, UnitName};
pub use layout::{Layout, Profile};
pub use toml::{Table, Toml, TomlDoc, TomlFile, TomlSchema, Validated};
pub use units::Units;
