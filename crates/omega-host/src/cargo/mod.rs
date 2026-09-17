//! Source-preserving Cargo documents.
//!
//! [`Manifest`] owns a `Cargo.toml`; [`Config`] owns a `.cargo/config.toml`.
//! Accessors read that same document, and edits retain unrelated keys and comments.
//! Parsing and editing perform no I/O. Use [`crate::Layout::file`] for atomic persistence.

mod config;
mod dependency;
mod fields;
mod manifest;
mod package;
mod pattern;
mod workspace;

pub use config::Config;
pub use dependency::{Dependencies, Dependency, DependencyDetail};
pub use fields::CargoError;
pub use manifest::{CargoSlot, Manifest};
pub use package::{Inherited, Package};
pub use pattern::{PathPattern, PatternError};
pub use workspace::{MembersError, Workspace};
