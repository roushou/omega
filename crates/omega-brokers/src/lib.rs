//! The subsystems Omega brokers.
//!
//! One broker per subsystem, holding one connection, reporting the topics
//! that subsystem owns and serving the actions that write to it. A broker
//! depends on the protocol and on its subsystem's client, and on nothing
//! else — the daemon is the crate that has to be readable end to end, and
//! D-Bus marshalling does not belong in it.
//!
//! Cadence is the broker's own. A subsystem with signals is woken by them
//! (`upower`); one without is polled (`backlight`). Both are the same shape
//! to the daemon, which is what lets it have one driver.
//!
//! Where a subsystem's numbers and the ontology's disagree — percent
//! against fraction, a state enum against a boolean — the translation is
//! split out as a plain function over a plain struct. The connection needs
//! a bus to test; the arithmetic is where the bugs are.
//!
//! Which broker covers what is the whole map: `tests/coverage.rs` compares
//! it against the ontology, so a topic or an action nothing serves is listed
//! there rather than discovered by a unit that hangs.

pub mod backlight;
mod broker;
pub mod clock;
pub mod hyprland;
pub mod logind;
pub mod network_manager;
pub mod upower;

pub use backlight::Backlight;
pub use broker::{Broker, BrokerError};
pub use clock::Clock;
pub use hyprland::Hyprland;
pub use logind::Logind;
pub use network_manager::NetworkManager;
pub use upower::UPower;

/// The brokers a daemon runs.
///
/// Boxed because the daemon holds them as one collection: it routes an
/// action to whichever broker claims the kind, and that lookup cannot be
/// monomorphised over a list that grows at the bottom of this file.
pub struct Brokers;

impl Brokers {
    /// Every broker, in the order they start.
    pub fn all() -> Vec<Box<dyn Broker>> {
        vec![
            Box::new(UPower::new()),
            Box::new(NetworkManager::new()),
            Box::new(Hyprland::new()),
            Box::new(Backlight::new()),
            Box::new(Logind::new()),
            Box::new(Clock::new()),
        ]
    }
}

impl std::fmt::Debug for Brokers {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Brokers")
    }
}
