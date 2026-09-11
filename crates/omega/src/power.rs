//! Battery charge, power sources, profiles, and peripheral batteries.
//!
//! Holding a state handle subscribes the plugin to its topics. Control handles
//! belong on commands or reactions; a widget cannot hold them.
//!
//! ```
//! use omega::power::Power;
//!
//! #[derive(omega::Widget)]
//! struct Indicator {
//!     power: Power,
//! }
//! ```

pub use crate::composite::{Power, Status};
pub use crate::effect::power_profile::SetProfile;
pub use crate::reading::{
    Battery, Mains, Peripheral, PeripheralKind, Peripherals, PowerProfile, PowerProfiles,
    ProfileLabel,
};
