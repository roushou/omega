//! Typed records shared through the daemon.
//!
//! Use [`Own`] to update records in commands or behavior and [`Watch`] to read
//! them in surfaces. `derive(UnitState)` uses the defining package as the owner
//! and derives a key from the type name. Records survive plugin restarts while
//! the daemon remains running; they are not persisted across daemon restarts.
//!
//! ```no_run
//! # use omega::record::{Own, Watch};
//! #[derive(omega::UnitState, Default, Clone)]
//! pub struct Power {
//!     pub on: bool,
//! }
//!
//! // In the plugin that owns it: read and write.
//! # #[derive(omega::Command)]
//! struct Toggle { power: Own<Power> }
//!
//! // In any plugin that named it: read.
//! # #[derive(omega::Surface)]
//! struct Lamp { power: Watch<Power> }
//! ```

use omega_proto::{Address, Fields};

/// A record stored in a plugin keyspace.
/// Derive `UnitState` to use the defining package name and the type-derived key.
pub trait UnitState: Fields + Send + Sync + 'static {
    /// The plugin whose keyspace this is.
    const UNIT: &'static str;
    /// The key within it.
    const KEY: &'static str;

    /// `unit.<unit>.<key>` — how the daemon addresses it.
    fn address() -> String {
        Address::of_unit(Self::UNIT, Self::KEY).to_string()
    }
}

mod own;
mod watch;
pub use own::Own;
pub use watch::Watch;
