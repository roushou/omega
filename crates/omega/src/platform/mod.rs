//! Typed access to external desktop services.
//!
//! Reading handles declare subscriptions. Controls declare capabilities and belong
//! to commands, reactions, or stateful behavior dependencies. Native service
//! implementations live in the daemon; this module adds no native dependencies.
//!
//! ```
//! use omega::platform::power::Battery;
//! #[derive(omega::Surface)]
//! struct Charge { battery: Battery }
//! ```

pub mod applications;
pub mod audio;
pub mod bluetooth;
pub mod desktop;
pub mod network;
pub mod notification;
pub mod power;
pub mod process;
pub mod session;
pub mod system;
pub mod time;

mod reading;
pub use reading::{Reading, ReadingError};
