//! The battery, from UPower.
//!
//! UPower's `DisplayDevice` is the composite answer — one battery on a laptop,
//! the sum of several where there are several, and a device that reports
//! itself absent on a machine with none. Reading it rather than enumerating
//! and picking is what keeps "the machine's battery" from being this broker's
//! opinion.
//!
//! Signal-driven: UPower marks every property `emits-change`, so a reading is
//! taken when one changes rather than on a timer. A battery that moves once a
//! minute costs one wake a minute instead of thirty.

use std::collections::HashMap;

use async_trait::async_trait;
use futures_util::StreamExt;
use zbus::fdo::PropertiesProxy;
use zbus::names::InterfaceName;
use zbus::zvariant::OwnedValue;
use zbus::{Connection, Proxy};

use omega_proto::SystemTopic;
use omega_proto::omega::{
    BatteryState, Peripheral, PeripheralKind, PeripheralsState, PowerState, StatePatch, StateTopic,
    state_topic,
};

use crate::broker::{Broker, BrokerError};

/// What UPower reports, as it reports it.
///
/// A plain struct rather than a handful of `get_property` calls at the point
/// of use: turning UPower's numbers into the ontology is the part that can be
/// wrong, and it is worth testing without a bus in the room.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Reading {
    pub is_present: bool,
    /// `UpDeviceKind`. 2 is a battery; a machine with none reports 0.
    pub kind: u32,
    /// `UpDeviceState`.
    pub state: u32,
    /// 0.0 .. 100.0, which the ontology carries as a fraction.
    pub percentage: f64,
    /// Seconds, or 0 for "not known" — which UPower also uses for "not
    /// applicable", so a charging battery reports 0 here.
    pub time_to_empty: i64,
    pub time_to_full: i64,
}

impl Reading {
    /// `UpDeviceKind::Battery`.
    const BATTERY: u32 = 2;
    /// `UpDeviceState::Charging`. Full-on-AC is `FullyCharged`, which is not
    /// charging: a widget that said otherwise would never stop saying it.
    const CHARGING: u32 = 1;

    /// The ontology's view, or `None` on a machine with no battery.
    ///
    /// `None` is not an error. A desktop has no battery, and the broker
    /// publishes the topic with no value so a unit can draw its no-reading
    /// branch instead of waiting for a value that never comes.
    pub fn state(&self) -> Option<BatteryState> {
        if !self.is_present || self.kind != Self::BATTERY {
            return None;
        }

        let charging = self.state == Self::CHARGING;
        Some(BatteryState {
            level: (self.percentage / 100.0).clamp(0.0, 1.0),
            charging,
            // UPower reports the one that does not apply as zero, and so does
            // the ontology — but only the applicable one is passed through, so
            // a discharging battery cannot report a time to full.
            seconds_to_empty: match charging {
                true => 0,
                false => Self::seconds(self.time_to_empty),
            },
            seconds_to_full: match charging {
                true => Self::seconds(self.time_to_full),
                false => 0,
            },
        })
    }

    /// UPower's seconds are signed and occasionally negative while it is
    /// still working an estimate out.
    fn seconds(value: i64) -> u32 {
        u32::try_from(value.max(0)).unwrap_or(u32::MAX)
    }

    fn from_properties(properties: &HashMap<String, OwnedValue>) -> Self {
        Self {
            is_present: Self::get(properties, "IsPresent").unwrap_or(false),
            kind: Self::get(properties, "Type").unwrap_or(0),
            state: Self::get(properties, "State").unwrap_or(0),
            percentage: Self::get(properties, "Percentage").unwrap_or(0.0),
            time_to_empty: Self::get(properties, "TimeToEmpty").unwrap_or(0),
            time_to_full: Self::get(properties, "TimeToFull").unwrap_or(0),
        }
    }

    /// A property UPower may not have. Missing reads as its default rather
    /// than as a failure: a device that omits `TimeToFull` is still a battery.
    fn get<T>(properties: &HashMap<String, OwnedValue>, name: &str) -> Option<T>
    where
        T: TryFrom<OwnedValue>,
    {
        T::try_from(properties.get(name)?.try_clone().ok()?).ok()
    }
}

/// One device that is not the machine itself.
///
/// UPower reports the laptop's own battery and everything plugged into it
/// through the same interface. What separates them is `Type`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Attached {
    pub path: String,
    pub model: String,
    pub kind: u32,
    pub percentage: f64,
    pub state: u32,
}

impl Attached {
    /// `UpDeviceKind`. Below `Mouse` are the machine's own supplies — the
    /// battery, the mains, a UPS — which the `battery` and `power` topics
    /// already answer for.
    const MOUSE: u32 = 5;

    /// Whether this is something plugged in rather than the machine itself.
    ///
    /// A device with no reading is dropped too: a keyboard that reports no
    /// battery is a keyboard, not a keyboard at zero percent.
    pub fn is_peripheral(&self) -> bool {
        self.kind >= Self::MOUSE && self.percentage > 0.0
    }

    fn peripheral(&self) -> Peripheral {
        Peripheral {
            // The object path, because a model is not unique — two identical
            // mice are two devices — and it survives a reconnect.
            id: self.path.rsplit('/').next().unwrap_or_default().to_string(),
            model: self.model.clone(),
            kind: self.peripheral_kind() as i32,
            percent: self.percentage.clamp(0.0, 100.0).round() as u32,
            charging: self.state == Reading::CHARGING,
        }
    }

    /// The kinds a bar draws differently. UPower knows thirty; the rest are
    /// `Other`, which still has a model and a percentage to show.
    fn peripheral_kind(&self) -> PeripheralKind {
        match self.kind {
            5 => PeripheralKind::Mouse,
            6 => PeripheralKind::Keyboard,
            10 => PeripheralKind::Tablet,
            8 => PeripheralKind::Phone,
            12 => PeripheralKind::GamingInput,
            17 | 19 => PeripheralKind::Headset,
            _ => PeripheralKind::Other,
        }
    }
}

/// What is plugged in, turned into the ontology.
#[derive(Debug)]
pub struct Peripherals;

impl Peripherals {
    /// Emptiest first would be arbitrary; a bar wants the one about to die at
    /// the top, and a stable order under it so the list does not shuffle.
    pub fn state(attached: &[Attached]) -> PeripheralsState {
        let mut mine: Vec<Peripheral> = attached
            .iter()
            .filter(|device| device.is_peripheral())
            .map(Attached::peripheral)
            .collect();

        mine.sort_by(|a, b| {
            a.percent
                .cmp(&b.percent)
                .then_with(|| a.model.cmp(&b.model))
        });
        PeripheralsState { devices: mine }
    }
}

/// The system bus, the device this broker watches, and the manager above it.
struct Link {
    device: PropertiesProxy<'static>,
    manager: Proxy<'static>,
    changes: zbus::fdo::PropertiesChangedStream,
}

impl Link {
    const SERVICE: &'static str = "org.freedesktop.UPower";
    const MANAGER: &'static str = "/org/freedesktop/UPower";
    const MANAGER_IFACE: &'static str = "org.freedesktop.UPower";
    const DEVICE: &'static str = "/org/freedesktop/UPower/devices/DisplayDevice";
    const INTERFACE: &'static str = "org.freedesktop.UPower.Device";

    async fn open() -> Result<Self, BrokerError> {
        let connection = Connection::system().await.map_err(Self::unreadable)?;

        // Named to be sure the service is there: a proxy is built lazily, so
        // without this a machine with no UPower would look connected and then
        // fail on every read.
        Proxy::new(&connection, Self::SERVICE, Self::DEVICE, Self::INTERFACE)
            .await
            .map_err(Self::unreadable)?;

        let manager = Proxy::new(
            &connection,
            Self::SERVICE,
            Self::MANAGER,
            Self::MANAGER_IFACE,
        )
        .await
        .map_err(Self::unreadable)?;

        let device = PropertiesProxy::builder(&connection)
            .destination(Self::SERVICE)
            .map_err(Self::unreadable)?
            .path(Self::DEVICE)
            .map_err(Self::unreadable)?
            .build()
            .await
            .map_err(Self::unreadable)?;

        let changes = device
            .receive_properties_changed()
            .await
            .map_err(Self::unreadable)?;

        Ok(Self {
            device,
            manager,
            changes,
        })
    }

    /// Every property in one call. Six round trips for one reading would be
    /// six chances for the answers to disagree with each other.
    async fn read(&self) -> Result<Reading, BrokerError> {
        let interface = InterfaceName::try_from(Self::INTERFACE).map_err(Self::unreadable)?;
        let properties = self
            .device
            .get_all(interface)
            .await
            .map_err(Self::unreadable)?;
        Ok(Reading::from_properties(&properties))
    }

    /// Everything UPower knows about, other than the composite device.
    ///
    /// A second walk rather than a second broker: it is one connection and
    /// one subsystem, and two brokers reading it would be two answers to one
    /// question.
    async fn attached(&self) -> Vec<Attached> {
        let paths: Vec<zbus::zvariant::OwnedObjectPath> = self
            .manager
            .call("EnumerateDevices", &())
            .await
            .unwrap_or_default();

        let mut found = Vec::new();
        for path in paths {
            let Ok(device) = Proxy::new(
                self.manager.connection(),
                Self::SERVICE,
                path.clone(),
                Self::INTERFACE,
            )
            .await
            else {
                continue;
            };

            found.push(Attached {
                path: path.as_str().to_string(),
                model: device.get_property("Model").await.unwrap_or_default(),
                kind: device.get_property("Type").await.unwrap_or(0),
                percentage: device.get_property("Percentage").await.unwrap_or(0.0),
                state: device.get_property("State").await.unwrap_or(0),
            });
        }
        found
    }

    /// Whether the machine is on mains.
    ///
    /// The manager's answer, not the battery's: a desktop has no battery and
    /// is still on mains, and asking the device would report nothing.
    async fn on_ac(&self) -> bool {
        !self
            .manager
            .get_property::<bool>("OnBattery")
            .await
            .unwrap_or(false)
    }

    fn unreadable(error: impl std::fmt::Display) -> BrokerError {
        BrokerError::Unreadable {
            subsystem: "upower",
            detail: error.to_string(),
        }
    }
}

impl std::fmt::Debug for Link {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Link")
    }
}

#[derive(Debug, Default)]
pub struct UPower {
    link: Option<Link>,
    /// Whether a reading has been taken on this connection. A fresh one
    /// reports what is true now rather than waiting for the next change,
    /// which on a full battery could be hours.
    ///
    /// Set only *after* a reading succeeds, so a `next` cancelled mid-read
    /// leaves the broker asking again rather than waiting on a change it has
    /// already missed the state of.
    primed: bool,
}

impl UPower {
    pub fn new() -> Self {
        Self::default()
    }

    /// Both topics in one patch. One power supply subsystem, one broker:
    /// two of them reading it would be two answers to one question.
    fn patch(
        state: Option<BatteryState>,
        on_ac: bool,
        peripherals: PeripheralsState,
    ) -> StatePatch {
        StatePatch {
            topics: vec![
                StateTopic {
                    topic: SystemTopic::Battery.as_str().into(),
                    revision: 0, // the Hub assigns the real revision
                    value: state.map(state_topic::Value::Battery),
                },
                StateTopic {
                    topic: SystemTopic::Power.as_str().into(),
                    revision: 0,
                    value: Some(state_topic::Value::Power(PowerState { on_ac })),
                },
                StateTopic {
                    topic: SystemTopic::Peripherals.as_str().into(),
                    revision: 0,
                    value: Some(state_topic::Value::Peripherals(peripherals)),
                },
            ],
        }
    }
}

#[async_trait]
impl Broker for UPower {
    fn name(&self) -> &'static str {
        "upower"
    }

    fn topics(&self) -> &'static [SystemTopic] {
        &[
            SystemTopic::Battery,
            SystemTopic::Power,
            SystemTopic::Peripherals,
        ]
    }

    async fn next(&mut self) -> Result<StatePatch, BrokerError> {
        if self.link.is_none() {
            self.link = Some(Link::open().await?);
            self.primed = false;
        }

        if self.primed {
            // `StreamExt::next` on a signal stream is cancel-safe: it is a
            // receiver, and dropping the future leaves what it had not taken.
            let alive = {
                let link = self.link.as_mut().expect("opened above");
                link.changes.next().await.is_some()
            };
            if !alive {
                self.link = None;
                self.primed = false;
                return Err(BrokerError::Unreadable {
                    subsystem: "upower",
                    detail: "the bus closed".into(),
                });
            }
        }

        let link = self.link.as_ref().expect("opened above");
        let reading = link.read().await?;
        let on_ac = link.on_ac().await;
        let attached = link.attached().await;
        self.primed = true;
        Ok(Self::patch(
            reading.state(),
            on_ac,
            Peripherals::state(&attached),
        ))
    }
}
