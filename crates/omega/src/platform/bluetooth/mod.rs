//! Bluetooth adapters, known devices, and connection control.

mod control;
mod devices;

pub use control::BluetoothControl;
pub use devices::{Bluetooth, BluetoothDevice, BluetoothStatus};

pub use omega_proto::{BluetoothDeviceId as DeviceId, BluetoothDeviceIdError as DeviceIdError};
