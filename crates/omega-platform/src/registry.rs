//! Default platform broker registry.

use crate::broker::Broker;
use crate::{
    Applications, Backlight, BlueZ, Clipboard, Clock, Desktop, Hwmon, Hyprland, Logind, Mpris,
    NetworkManager, Notifications, PipeWire, PowerProfiles, Procfs, UPower,
};

/// Construct the daemon's platform brokers.
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
            Box::new(Hwmon::new()),
            Box::new(Desktop::new()),
            Box::new(Clipboard::new()),
            Box::new(Applications::new()),
        ]
    }
}

impl std::fmt::Debug for Brokers {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Brokers")
    }
}
