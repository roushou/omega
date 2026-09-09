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
//! A broker says how to [`connect`], [`wake`] and [`read`]; the daemon's
//! driver says when. The rules about holding a connection live there once
//! rather than in twelve loops — see [`Broker`].
//!
//! [`connect`]: Broker::connect
//! [`wake`]: Broker::wake
//! [`read`]: Broker::read
//!
//! Which broker covers what is the whole map: `tests/coverage.rs` compares
//! it against the ontology, so a topic or an action nothing serves is listed
//! there rather than discovered by a unit that hangs.

pub mod backlight;
pub mod bluez;
mod broker;
pub mod clock;
mod dbus;
pub mod desktop;
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
pub use clock::Clock;
pub use desktop::Desktop;
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
