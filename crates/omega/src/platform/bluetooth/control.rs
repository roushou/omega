//! Connecting known Bluetooth devices through the daemon.
use crate::{effect::Effect, runtime::context::Context, wiring::does};
use omega_proto::{
    BluetoothDeviceId,
    omega::{ConnectBluetooth, DisconnectBluetooth, action},
};

/// Permission to connect and disconnect known devices. Pairing is separate.
/// Completion acknowledges BlueZ's reply; the reading reports connection state.
///
/// ```no_run
/// use omega::{Command, platform::bluetooth::{BluetoothControl, DeviceId}};
/// #[derive(omega::Command)]
/// struct Connect { bluetooth: BluetoothControl }
/// impl Command for Connect {
///     type Input = DeviceId;
///     type Output = ();
///     async fn call(&self, id: DeviceId) -> omega::Result<()> {
///         self.bluetooth.connect(&id).await
///     }
/// }
/// ```
///
/// ```compile_fail
/// #[derive(omega::Surface)]
/// struct Panel { bluetooth: omega::platform::bluetooth::BluetoothControl }
/// ```
#[derive(Debug)]
pub struct BluetoothControl {
    context: Context,
}
does!(BluetoothControl, Bluetooth);
impl BluetoothControl {
    /// Connect a paired device on its own adapter. Never scans, pairs, or retries.
    pub fn connect(&self, id: &BluetoothDeviceId) -> Effect {
        self.act(action::Kind::ConnectBluetooth(ConnectBluetooth {
            device_id: id.to_string(),
        }))
    }
    /// Disconnect this endpoint without changing the adapter or other devices.
    pub fn disconnect(&self, id: &BluetoothDeviceId) -> Effect {
        self.act(action::Kind::DisconnectBluetooth(DisconnectBluetooth {
            device_id: id.to_string(),
        }))
    }
}
