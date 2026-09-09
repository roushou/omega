//! Which brokers a daemon runs.
//!
//! A list, and the only place there is one. `omega-cli` wires the daemon
//! together; which subsystems that daemon brokers is this crate's to say.

use crate::broker::Broker;
use crate::{
    Backlight, BlueZ, Clock, Desktop, Hyprland, Logind, Mpris, NetworkManager, Notifications,
    PipeWire, PowerProfiles, Procfs, UPower,
};

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
            Box::new(PowerProfiles::new()),
            Box::new(NetworkManager::new()),
            Box::new(Hyprland::new()),
            Box::new(Backlight::new()),
            Box::new(Logind::new()),
            Box::new(Clock::new()),
            Box::new(Notifications::new()),
            Box::new(PipeWire::new()),
            Box::new(Mpris::new()),
            Box::new(BlueZ::new()),
            Box::new(Procfs::new()),
            Box::new(Desktop::new()),
        ]
    }
}

impl std::fmt::Debug for Brokers {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Brokers")
    }
}
