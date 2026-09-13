//! The Bluetooth adapter and its devices, from BlueZ.
//!
//! One `GetManagedObjects` call answers everything BlueZ knows — the adapter
//! and every device under it, with all their interfaces — so a reading is one
//! round trip rather than a walk.
//!
//! Property and object signals wake the broker; polling also reconciles state.

use std::collections::HashMap;
use std::time::Duration;

use async_trait::async_trait;
use futures_util::StreamExt;
use zbus::zvariant::{OwnedObjectPath, OwnedValue};
use zbus::{Connection, MatchRule, MessageStream, Proxy};

use omega_proto::omega::{
    BluetoothDevice, BluetoothState, StatePatch, StateTopic, action, state_topic,
};
use omega_proto::{ActionKind, BluetoothDeviceId, SystemTopic};

use crate::broker::{Broker, BrokerError, Cadence, opaque_debug};
use crate::dbus;

/// The adapter, as `org.bluez.Adapter1` describes it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Adapter {
    pub powered: bool,
    pub discovering: bool,
}

/// One device, as `org.bluez.Device1` describes it.
#[derive(Debug, Clone, PartialEq)]
pub struct Device {
    pub id: BluetoothDeviceId,
    pub can_connect: bool,
    pub address: String,
    /// The alias, which is what a person renamed it to. BlueZ falls back to
    /// the name the device broadcasts, so this is always the better one.
    pub alias: String,
    pub connected: bool,
    pub paired: bool,
    pub icon: String,
    /// From `org.bluez.Battery1`, which only exists on a connected device
    /// that has one.
    pub battery: Option<u8>,
}

/// What BlueZ answered, turned into the ontology.
#[derive(Debug)]
pub struct Objects;

impl Objects {
    /// Resolve only the requested known endpoint; never select another adapter.
    pub fn target<'a>(
        devices: &'a [Device],
        id: &BluetoothDeviceId,
        connect: bool,
    ) -> Result<&'a Device, BrokerError> {
        let device = devices
            .iter()
            .find(|device| &device.id == id && (device.paired || device.connected))
            .ok_or_else(|| {
                BrokerError::Unreadable("Bluetooth device is no longer available".into())
            })?;
        if connect && !device.can_connect {
            return Err(BrokerError::Unreadable("Bluetooth device cannot connect: it must be paired, unblocked, and its adapter powered".into()));
        }
        Ok(device)
    }

    /// The machine's own devices, connected first.
    ///
    /// Only what is paired or connected. BlueZ also lists whatever it has
    /// seen recently, and a bar showing every phone that walked past is
    /// showing the air rather than the machine.
    pub fn state(adapter: Option<&Adapter>, devices: &[Device]) -> BluetoothState {
        let mut mine: Vec<&Device> = devices
            .iter()
            .filter(|device| device.paired || device.connected)
            .collect();

        // Connected first, then by name: BlueZ answers in whatever order it
        // holds paths, and a list that reordered itself would move under the
        // cursor.
        mine.sort_by(|a, b| {
            b.connected
                .cmp(&a.connected)
                .then_with(|| a.alias.cmp(&b.alias))
                .then_with(|| a.id.cmp(&b.id))
        });

        BluetoothState {
            available: adapter.is_some(),
            powered: adapter.is_some_and(|adapter| adapter.powered),
            discovering: adapter.is_some_and(|adapter| adapter.discovering),
            devices: mine
                .into_iter()
                .map(|device| BluetoothDevice {
                    id: device.id.to_string(),
                    can_connect: device.can_connect,
                    address: device.address.clone(),
                    name: device.alias.clone(),
                    connected: device.connected,
                    paired: device.paired,
                    icon: device.icon.clone(),
                    battery_percent: device.battery.map(u32::from),
                })
                .collect(),
        }
    }
}

#[zbus::proxy(interface = "org.bluez.Device1", default_service = "org.bluez")]
trait DeviceControl {
    fn connect(&self) -> zbus::Result<()>;
    fn disconnect(&self) -> zbus::Result<()>;
}

/// The system bus, and the subscription to everything BlueZ says.
struct Link {
    connection: Connection,
    changes: MessageStream,
}

impl Link {
    const SERVICE: &'static str = "org.bluez";
    const ADAPTER: &'static str = "org.bluez.Adapter1";
    const DEVICE: &'static str = "org.bluez.Device1";
    const BATTERY: &'static str = "org.bluez.Battery1";

    async fn open() -> Result<Self, BrokerError> {
        let connection = Connection::system()
            .await
            .map_err(BrokerError::unreadable)?;

        // Everything BlueZ says, rather than a rule per interface: an adapter
        // powering on and a headset connecting are the same kind of news.
        let rule = MatchRule::builder()
            .msg_type(zbus::message::Type::Signal)
            .sender(Self::SERVICE)
            .map_err(BrokerError::unreadable)?
            .build();

        let changes = MessageStream::for_match_rule(rule, &connection, None)
            .await
            .map_err(BrokerError::unreadable)?;

        Ok(Self {
            connection,
            changes,
        })
    }

    /// Everything BlueZ knows, in one call.
    async fn read(&self) -> Result<(Option<Adapter>, Vec<Device>), BrokerError> {
        let manager = Proxy::new(
            &self.connection,
            Self::SERVICE,
            "/",
            "org.freedesktop.DBus.ObjectManager",
        )
        .await
        .map_err(BrokerError::unreadable)?;

        type Managed = HashMap<OwnedObjectPath, HashMap<String, HashMap<String, OwnedValue>>>;
        let objects: Managed = manager
            .call("GetManagedObjects", &())
            .await
            .map_err(BrokerError::unreadable)?;

        let mut adapter = None;
        let mut devices = Vec::new();

        for interfaces in objects.values() {
            if let Some(properties) = interfaces.get(Self::ADAPTER) {
                let found = adapter.get_or_insert(Adapter::default());
                found.powered |= dbus::field(properties, "Powered").unwrap_or(false);
                found.discovering |= dbus::field(properties, "Discovering").unwrap_or(false);
            }
        }
        for (path, interfaces) in &objects {
            if let Some(properties) = interfaces.get(Self::DEVICE) {
                let id =
                    BluetoothDeviceId::parse(path.to_string()).map_err(BrokerError::unreadable)?;
                let adapter_path: OwnedObjectPath =
                    dbus::field(properties, "Adapter").ok_or_else(|| {
                        BrokerError::Unreadable(format!("device {id} has no adapter"))
                    })?;
                let powered = objects
                    .get(&adapter_path)
                    .and_then(|interfaces| interfaces.get(Self::ADAPTER))
                    .and_then(|properties| dbus::field::<bool>(properties, "Powered"))
                    .unwrap_or(false);
                let paired = dbus::field(properties, "Paired").unwrap_or(false);
                let blocked: bool = dbus::field(properties, "Blocked").unwrap_or(false);
                devices.push(Device {
                    id,
                    can_connect: paired && powered && !blocked,
                    address: dbus::field(properties, "Address").unwrap_or_default(),
                    alias: dbus::field(properties, "Alias").unwrap_or_default(),
                    connected: dbus::field(properties, "Connected").unwrap_or(false),
                    paired,
                    icon: dbus::field(properties, "Icon").unwrap_or_default(),
                    battery: interfaces
                        .get(Self::BATTERY)
                        .and_then(|battery| dbus::field(battery, "Percentage")),
                });
            }
        }

        Ok((adapter, devices))
    }
    async fn call(&self, id: &BluetoothDeviceId, connect: bool) -> Result<(), BrokerError> {
        let device = DeviceControlProxy::builder(&self.connection)
            .path(id.as_str())
            .map_err(BrokerError::unreadable)?
            .build()
            .await
            .map_err(BrokerError::unreadable)?;
        if connect {
            device.connect().await
        } else {
            device.disconnect().await
        }
        .map_err(BrokerError::unreadable)
    }
}

opaque_debug!(Link);

#[derive(Debug)]
pub struct BlueZ {
    link: Option<Link>,
    tick: Cadence,
}

impl Default for BlueZ {
    fn default() -> Self {
        Self::new()
    }
}

impl BlueZ {
    /// Periodic reconciliation supplements BlueZ signals.
    pub const REFRESH: Duration = Duration::from_secs(10);

    pub fn new() -> Self {
        Self {
            link: None,
            tick: Cadence::after(Self::REFRESH),
        }
    }

    fn patch(state: BluetoothState) -> StatePatch {
        StatePatch {
            topics: vec![StateTopic {
                topic: SystemTopic::Bluetooth.as_str().into(),
                revision: 0, // the Hub assigns the real revision
                value: Some(state_topic::Value::Bluetooth(state)),
            }],
        }
    }
}

#[async_trait]
impl Broker for BlueZ {
    fn name(&self) -> &'static str {
        "bluez"
    }

    fn topics(&self) -> &'static [SystemTopic] {
        &[SystemTopic::Bluetooth]
    }

    fn actions(&self) -> &'static [ActionKind] {
        &[
            ActionKind::ConnectBluetooth,
            ActionKind::DisconnectBluetooth,
        ]
    }

    async fn act(&mut self, action: &action::Kind) -> Result<Option<StatePatch>, BrokerError> {
        let (id, connect) = match action {
            action::Kind::ConnectBluetooth(target) => (&target.device_id, true),
            action::Kind::DisconnectBluetooth(target) => (&target.device_id, false),
            _ => return Err(BrokerError::Unserved(ActionKind::of(action))),
        };
        let id = BluetoothDeviceId::parse(id.clone()).map_err(BrokerError::unreadable)?;
        let link = self.link.as_ref().ok_or_else(BrokerError::gone)?;
        let (_, devices) = link.read().await?;
        let target = Objects::target(&devices, &id, connect)?;
        link.call(&target.id, connect).await?;
        Ok(None)
    }

    fn disconnect(&mut self) {
        self.link = None;
    }

    async fn connect(&mut self) -> Result<(), BrokerError> {
        self.link = Some(Link::open().await?);
        Ok(())
    }

    async fn wake(&mut self) -> Result<(), BrokerError> {
        let Self { link, tick, .. } = self;
        let link = link.as_mut().ok_or_else(BrokerError::gone)?;
        // Both arms are cancel-safe: a signal stream is a receiver, and an
        // interval keeps its own deadline.
        tokio::select! {
            change = link.changes.next() => match change {
                Some(_) => Ok(()),
                None => Err(BrokerError::gone()),
            },
            _ = tick.wait() => Ok(()),
        }
    }

    async fn read(&mut self) -> Result<StatePatch, BrokerError> {
        let link = self.link.as_ref().ok_or_else(BrokerError::gone)?;
        let (adapter, devices) = link.read().await?;
        Ok(Self::patch(Objects::state(adapter.as_ref(), &devices)))
    }
}
