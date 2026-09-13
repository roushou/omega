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

pub use crate::reading::{Bluetooth, BluetoothDevice, BluetoothStatus};

pub use crate::effect::bluetooth::BluetoothControl;
/// A device's identity across readings, bindings, and command inputs.
///
/// ```
/// use omega::bluetooth::DeviceId;
/// let id = DeviceId::parse("/org/bluez/hci0/dev_60_AB_D2_25_8C_49").unwrap();
/// assert!(id.as_str().contains("hci0"));
/// ```
pub use omega_proto::BluetoothDeviceId as DeviceId;
pub use omega_proto::BluetoothDeviceIdError as DeviceIdError;
