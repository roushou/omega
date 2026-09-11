//! Bluetooth adapters and paired devices.
//!
//! Holding a state handle subscribes the plugin to its topics. Control handles
//! belong on commands or reactions; a widget cannot hold them.
//!
//! ```
//! use omega::bluetooth::Bluetooth;
//!
//! #[derive(omega::Widget)]
//! struct Indicator {
//!     bluetooth: Bluetooth,
//! }
//! ```

pub use crate::reading::{Bluetooth, BluetoothDevice};
