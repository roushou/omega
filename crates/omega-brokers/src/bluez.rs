//! The Bluetooth adapter and its devices, from BlueZ.
//!
//! One `GetManagedObjects` call answers everything BlueZ knows — the adapter
//! and every device under it, with all their interfaces — so a reading is one
//! round trip rather than a walk.
//!
//! Woken by signals and polled underneath. BlueZ reports a property changing;
//! a device being paired or forgotten is an object appearing or going, which
//! the floor notices.

use std::collections::HashMap;
use std::time::Duration;

use async_trait::async_trait;
use futures_util::StreamExt;
use zbus::zvariant::{OwnedObjectPath, OwnedValue};
use zbus::{Connection, MatchRule, MessageStream, Proxy};

use omega_proto::SystemTopic;
use omega_proto::omega::{BluetoothDevice, BluetoothState, StatePatch, StateTopic, state_topic};

use crate::broker::{Broker, BrokerError, Cadence};

/// The adapter, as `org.bluez.Adapter1` describes it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Adapter {
    pub powered: bool,
    pub discovering: bool,
}

/// One device, as `org.bluez.Device1` describes it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Device {
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
        });

        BluetoothState {
            available: adapter.is_some(),
            powered: adapter.is_some_and(|adapter| adapter.powered),
            discovering: adapter.is_some_and(|adapter| adapter.discovering),
            devices: mine
                .into_iter()
                .map(|device| BluetoothDevice {
                    address: device.address.clone(),
                    name: device.alias.clone(),
                    connected: device.connected,
                    paired: device.paired,
                    icon: device.icon.clone(),
                    battery_percent: u32::from(device.battery.unwrap_or(0)),
                })
                .collect(),
        }
    }
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
        let connection = Connection::system().await.map_err(Self::unreadable)?;

        // Everything BlueZ says, rather than a rule per interface: an adapter
        // powering on and a headset connecting are the same kind of news.
        let rule = MatchRule::builder()
            .msg_type(zbus::message::Type::Signal)
            .sender(Self::SERVICE)
            .map_err(Self::unreadable)?
            .interface("org.freedesktop.DBus.Properties")
            .map_err(Self::unreadable)?
            .member("PropertiesChanged")
            .map_err(Self::unreadable)?
            .build();

        let changes = MessageStream::for_match_rule(rule, &connection, None)
            .await
            .map_err(Self::unreadable)?;

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
        .map_err(Self::unreadable)?;

        type Managed = HashMap<OwnedObjectPath, HashMap<String, HashMap<String, OwnedValue>>>;
        let objects: Managed = manager
            .call("GetManagedObjects", &())
            .await
            .map_err(Self::unreadable)?;

        let mut adapter = None;
        let mut devices = Vec::new();

        for interfaces in objects.values() {
            if let Some(properties) = interfaces.get(Self::ADAPTER) {
                // The first adapter. A machine with two is rare enough that
                // picking one is better than inventing a way to choose.
                adapter.get_or_insert(Adapter {
                    powered: Self::get(properties, "Powered").unwrap_or(false),
                    discovering: Self::get(properties, "Discovering").unwrap_or(false),
                });
            }

            if let Some(properties) = interfaces.get(Self::DEVICE) {
                devices.push(Device {
                    address: Self::get(properties, "Address").unwrap_or_default(),
                    alias: Self::get(properties, "Alias").unwrap_or_default(),
                    connected: Self::get(properties, "Connected").unwrap_or(false),
                    paired: Self::get(properties, "Paired").unwrap_or(false),
                    icon: Self::get(properties, "Icon").unwrap_or_default(),
                    // On the same object, so no second call: BlueZ puts the
                    // battery interface on a device that has one.
                    battery: interfaces
                        .get(Self::BATTERY)
                        .and_then(|battery| Self::get(battery, "Percentage")),
                });
            }
        }

        Ok((adapter, devices))
    }

    fn get<T>(properties: &HashMap<String, OwnedValue>, name: &str) -> Option<T>
    where
        T: TryFrom<OwnedValue>,
    {
        T::try_from(properties.get(name)?.try_clone().ok()?).ok()
    }

    fn unreadable(error: impl std::fmt::Display) -> BrokerError {
        BrokerError::Unreadable {
            subsystem: "bluez",
            detail: error.to_string(),
        }
    }
}

impl std::fmt::Debug for Link {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Link")
    }
}

#[derive(Debug)]
pub struct BlueZ {
    link: Option<Link>,
    tick: Cadence,
    primed: bool,
}

impl Default for BlueZ {
    fn default() -> Self {
        Self::new()
    }
}

impl BlueZ {
    /// The floor under the signals: a device being paired or forgotten is an
    /// object appearing or going, which no property change reports.
    pub const REFRESH: Duration = Duration::from_secs(10);

    pub fn new() -> Self {
        Self {
            link: None,
            tick: Cadence::after(Self::REFRESH),
            primed: false,
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

    async fn next(&mut self) -> Result<StatePatch, BrokerError> {
        if self.link.is_none() {
            self.link = Some(Link::open().await?);
            self.primed = false;
        }

        if self.primed {
            let closed = {
                let Self { link, tick, .. } = self;
                let link = link.as_mut().expect("opened above");
                tokio::select! {
                    change = link.changes.next() => change.is_none(),
                    _ = tick.wait() => false,
                }
            };
            if closed {
                self.link = None;
                self.primed = false;
                return Err(BrokerError::Unreadable {
                    subsystem: "bluez",
                    detail: "the bus closed".into(),
                });
            }
        }

        let (adapter, devices) = self.link.as_ref().expect("opened above").read().await?;
        self.primed = true;
        Ok(Self::patch(Objects::state(adapter.as_ref(), &devices)))
    }
}
