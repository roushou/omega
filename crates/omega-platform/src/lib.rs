//! Linux subsystem brokers for state readings and control actions.
//! Implementations own connections and protocol conversion. The daemon driver
//! controls connection attempts, wakeups, reads, and retries.
//! Coverage tests verify declared topics and action handlers.

pub mod applications;
pub use applications::Applications;
pub mod backlight;
pub mod bluez;
mod broker;
pub mod clipboard;
pub mod clock;
mod dbus;
pub mod desktop;
pub mod hwmon;
pub mod hyprland;
pub mod logind;
pub mod mpris;
pub mod network_manager;
pub mod notifications;
pub mod pipewire;
pub mod power_profiles;
pub mod procfs;
mod registry;
pub mod upower;

pub use backlight::Backlight;
pub use bluez::BlueZ;
pub use broker::{Broker, BrokerError};
pub use clipboard::Clipboard;
pub use clock::Clock;
pub use desktop::Desktop;
pub use hwmon::Hwmon;
pub use hyprland::Hyprland;
pub use logind::Logind;
pub use mpris::Mpris;
pub use network_manager::NetworkManager;
pub use notifications::Notifications;
pub use pipewire::PipeWire;
pub use power_profiles::PowerProfiles;
pub use procfs::Procfs;
pub use registry::Brokers;
pub use upower::UPower;
