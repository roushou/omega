//! System resources, storage, temperatures, and supervised Omega units.
//!
//! Holding a state handle subscribes the plugin to its topics. Control handles
//! belong on commands or reactions; a widget cannot hold them.
//!
//! ```
//! use omega::system::System;
//!
//! #[derive(omega::Widget)]
//! struct Indicator {
//!     system: System,
//! }
//! ```

pub use crate::reading::{
    Disk, Fan, Load, Memory, Mount, Sensor, System, Thermals, UnitPhase, UnitReport, Units,
};
