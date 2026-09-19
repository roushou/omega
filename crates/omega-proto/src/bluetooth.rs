//! Bluetooth device identities include the owning adapter.
use crate::{FromValue, IntoValue, omega::Value};
use std::fmt;

/// An opaque BlueZ device endpoint. An address alone is ambiguous across adapters.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BluetoothDeviceId(String);
impl std::str::FromStr for BluetoothDeviceId {
    type Err = BluetoothDeviceIdError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::try_from(value.to_owned())
    }
}

impl TryFrom<&str> for BluetoothDeviceId {
    type Error = BluetoothDeviceIdError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl TryFrom<String> for BluetoothDeviceId {
    type Error = BluetoothDeviceIdError;

    /// Validate a device endpoint obtained from a Bluetooth reading.
    fn try_from(id: String) -> Result<Self, Self::Error> {
        let valid = id
            .strip_prefix("/org/bluez/hci")
            .and_then(|tail| tail.split_once("/dev_"))
            .is_some_and(|(adapter, address)| {
                !adapter.is_empty()
                    && adapter.bytes().all(|c| c.is_ascii_digit())
                    && address.len() == 17
                    && address.split('_').count() == 6
                    && address.split('_').all(|part| {
                        part.len() == 2
                            && part
                                .bytes()
                                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_lowercase())
                    })
            });
        if !valid {
            return Err(BluetoothDeviceIdError(id));
        }
        Ok(Self(id))
    }
}

impl BluetoothDeviceId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for BluetoothDeviceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid Bluetooth device id: {0:?}")]
pub struct BluetoothDeviceIdError(String);
impl FromValue for BluetoothDeviceId {
    fn from_value(value: &Value) -> Option<Self> {
        Self::try_from(String::from_value(value)?).ok()
    }
}

impl IntoValue for BluetoothDeviceId {
    fn into_value(self) -> Value {
        self.0.into_value()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ActionKind,
        omega::{Capability, ConnectBluetooth, DisconnectBluetooth, action},
    };
    #[test]
    fn controls_require_bluetooth_permission_and_valid_device_endpoints() {
        for id in [
            "/org/bluez/hci0",
            "/org/bluez/hci/dev_60_AB_D2_25_8C_49",
            "/org/bluez/hci0/dev_60_AB_D2_25_8C_49/other",
            "60:AB:D2:25:8C:49",
            "",
        ] {
            assert!(BluetoothDeviceId::try_from(id).is_err());
            assert!(
                action::Kind::ConnectBluetooth(ConnectBluetooth {
                    device_id: id.into()
                })
                .validate()
                .is_err()
            );
            assert!(
                action::Kind::DisconnectBluetooth(DisconnectBluetooth {
                    device_id: id.into()
                })
                .validate()
                .is_err()
            );
        }
        assert_eq!(
            ActionKind::ConnectBluetooth.cost(),
            Some(Capability::Bluetooth)
        );
        assert_eq!(
            ActionKind::DisconnectBluetooth.cost(),
            Some(Capability::Bluetooth)
        );
        let id = "/org/bluez/hci12/dev_60_AB_D2_25_8C_49"
            .parse::<BluetoothDeviceId>()
            .unwrap();
        assert_eq!(
            BluetoothDeviceId::from_value(&id.clone().into_value()),
            Some(id)
        );
        assert!(crate::Handshake::negotiate(crate::PROTOCOL_VERSION + 1).is_err());
    }
}
