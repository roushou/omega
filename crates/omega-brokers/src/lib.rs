//! The subsystems Omega brokers.
//!
//! One broker per subsystem, holding one connection, reporting the topics
//! that subsystem owns and serving the actions that write to it. A broker
//! depends on the protocol and on its subsystem's client, and on nothing
//! else — the daemon is the crate that has to be readable end to end, and
//! D-Bus marshalling does not belong in it.
//!
//! Which broker covers what is the whole map: `tests/coverage.rs` compares
//! it against the ontology, so a topic or an action nothing serves is listed
//! there rather than discovered by a unit that hangs.

mod backlight;
mod battery;
mod broker;

pub use backlight::Backlight;
pub use battery::Battery;
pub use broker::{Broker, BrokerError};

/// The brokers a daemon runs.
///
/// Boxed because the daemon holds them as one collection: it routes an
/// action to whichever broker claims the kind, and that lookup cannot be
/// monomorphised over a list that grows at the bottom of this file.
pub struct Brokers;

impl Brokers {
    /// Every broker, in the order they start.
    pub fn all() -> Vec<Box<dyn Broker>> {
        vec![Box::new(Battery::new()), Box::new(Backlight::new())]
    }
}

impl std::fmt::Debug for Brokers {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Brokers")
    }
}
