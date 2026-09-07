//! What a build emits and the daemon reads back: a unit's manifest, and the
//! listing of the units a build produced.
//!
//! Both are documents that cross the boundary between the CLI that writes
//! them and the daemon that trusts them, and both are validated before
//! anything acts on them.

mod error;
mod manifest;
mod state;

pub use error::ManifestError;
pub use manifest::{Manifest, Surface};
pub use omega_core::UnitName;
pub use state::{BuiltUnit, StateConfig};
