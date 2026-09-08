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
use omega_proto::omega::{BatteryState, PowerState, StatePatch, StateTopic, state_topic};

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
    fn patch(state: Option<BatteryState>, on_ac: bool) -> StatePatch {
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
        &[SystemTopic::Battery, SystemTopic::Power]
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
        self.primed = true;
        Ok(Self::patch(reading.state(), on_ac))
    }
}
