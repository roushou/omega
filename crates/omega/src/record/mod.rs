//! State a plugin owns.
//!
//! The daemon replicates more than the machine's own topics: every plugin has
//! a keyspace of its own, `unit.<name>.<key>`, which it writes and subscribing
//! plugins may read. Typed consumers depend on the crate defining the record;
//! the daemon mediates publication and observation.
//!
//! The topic's address comes from the type. A state type is defined in the
//! crate that owns it, so `#[derive(UnitState)]` reads the unit's name from
//! that crate and the key from the type — and a reader writes
//! `Watch<lamp::Power>` rather than a string that a rename would quietly
//! break.
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

/// A value that lives in a plugin's keyspace.
///
/// Derived. The unit is the crate that defines the type, and the key is the
/// type's own name, so neither is a string anybody types.
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
