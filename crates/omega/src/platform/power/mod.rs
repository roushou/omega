//! Battery charge, power sources, profiles, and peripheral batteries.

mod battery;
mod mains;
mod peripherals;
mod profiles;
mod status;

pub use battery::Battery;
pub use mains::Mains;
pub use peripherals::{Peripheral, PeripheralKind, Peripherals};
pub use profiles::{PowerProfile, PowerProfiles, ProfileLabel, SetProfile};
pub use status::{Power, Status};
