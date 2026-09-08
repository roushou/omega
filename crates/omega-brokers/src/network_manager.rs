//! The network, from NetworkManager.
//!
//! NetworkManager answers "what is this machine on" through a chain: the
//! manager names a primary connection, the connection names a device, and a
//! wireless device names the access point it is associated with. Walking it
//! here is what keeps every widget from walking it itself.
//!
//! Woken by signals *and* polled. `PropertiesChanged` on the manager reports
//! connecting and disconnecting the moment they happen, but signal strength
//! lives on the access point and changes as somebody walks around — so a slow
//! tick is the floor under the signals rather than a replacement for them.

use async_trait::async_trait;
use futures_util::StreamExt;
use std::time::Duration;
use zbus::fdo::PropertiesProxy;
use zbus::zvariant::OwnedObjectPath;
use zbus::{Connection, Proxy};

use omega_proto::SystemTopic;
use omega_proto::omega::{NetworkState, NetworkType, StatePatch, StateTopic, state_topic};

use crate::broker::{Broker, BrokerError, Cadence};

/// What NetworkManager reports, as it reports it.
///
/// Split from the walk that gathers it because the walk needs a bus and this
/// does not: NetworkManager's vocabulary and the ontology's disagree about
/// almost everything, and that is where the mistakes are.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Reading {
    /// `NMState`. 70 is connected with internet, 20 disconnected.
    pub manager_state: u32,
    /// The connection's `Type`: `"802-11-wireless"`, `"802-3-ethernet"`, …
    pub kind: String,
    /// The connection's name. For Wi-Fi it is usually the SSID, but it is
    /// the *connection's* name and somebody may have renamed it.
    pub id: String,
    pub interface: String,
    /// From the associated access point, which is authoritative where the
    /// connection's name is only conventional.
    pub ssid: String,
    /// 0..100, from the access point.
    pub strength: u8,
    pub is_vpn: bool,
}

impl Reading {
    /// `NM_STATE_CONNECTED_LOCAL`. Below this the machine is not on a network;
    /// above it, it is on one that may or may not reach the internet — which
    /// is a different question, and not the one this field asks.
    const CONNECTED: u32 = 50;

    /// The ontology's view.
    ///
    /// Always a reading, never absent: NetworkManager answering at all means
    /// there is something true to say, and "disconnected" is one of the things
    /// it can be. Absence is for a machine with no NetworkManager, which is
    /// the broker failing to connect rather than a reading of nothing.
    pub fn state(&self) -> NetworkState {
        NetworkState {
            connected: self.manager_state >= Self::CONNECTED,
            ssid: self.name(),
            interface: self.interface.clone(),
            signal_percent: u32::from(self.strength),
            r#type: self.kind() as i32,
        }
    }

    /// What to call the network.
    ///
    /// The access point where there is one, because a connection can be
    /// renamed and the SSID cannot. The connection's own name is the fallback
    /// rather than nothing: a Wi-Fi widget with a blank label while the
    /// association settles looks broken.
    fn name(&self) -> String {
        match self.ssid.is_empty() {
            false => self.ssid.clone(),
            true => self.id.clone(),
        }
    }

    fn kind(&self) -> NetworkType {
        // A VPN is reported as the primary connection when one is up, which
        // is why it is checked first. The cost is that the network it runs
        // over is not also reported — "on Wi-Fi *and* a VPN" needs a topic of
        // its own, not another arm of this enum.
        if self.is_vpn {
            return NetworkType::Vpn;
        }
        match self.kind.as_str() {
            "802-11-wireless" => NetworkType::Wifi,
            "802-3-ethernet" => NetworkType::Ethernet,
            "gsm" | "cdma" | "wwan" => NetworkType::Cellular,
            "vpn" | "wireguard" => NetworkType::Vpn,
            _ => NetworkType::Unspecified,
        }
    }
}

/// The system bus, and the manager this broker walks from.
struct Link {
    connection: Connection,
    manager: Proxy<'static>,
    changes: zbus::fdo::PropertiesChangedStream,
}

impl Link {
    const SERVICE: &'static str = "org.freedesktop.NetworkManager";
    const MANAGER: &'static str = "/org/freedesktop/NetworkManager";
    const MANAGER_IFACE: &'static str = "org.freedesktop.NetworkManager";
    const ACTIVE_IFACE: &'static str = "org.freedesktop.NetworkManager.Connection.Active";
    const DEVICE_IFACE: &'static str = "org.freedesktop.NetworkManager.Device";
    const WIRELESS_IFACE: &'static str = "org.freedesktop.NetworkManager.Device.Wireless";
    const AP_IFACE: &'static str = "org.freedesktop.NetworkManager.AccessPoint";

    /// The object path NetworkManager uses for "there is none".
    const NONE: &'static str = "/";

    async fn open() -> Result<Self, BrokerError> {
        let connection = Connection::system().await.map_err(Self::unreadable)?;
        let manager = Proxy::new(
            &connection,
            Self::SERVICE,
            Self::MANAGER,
            Self::MANAGER_IFACE,
        )
        .await
        .map_err(Self::unreadable)?;

        let properties = PropertiesProxy::builder(&connection)
            .destination(Self::SERVICE)
            .map_err(Self::unreadable)?
            .path(Self::MANAGER)
            .map_err(Self::unreadable)?
            .build()
            .await
            .map_err(Self::unreadable)?;
        let changes = properties
            .receive_properties_changed()
            .await
            .map_err(Self::unreadable)?;

        Ok(Self {
            connection,
            manager,
            changes,
        })
    }

    /// Walk from the manager to the access point, taking what is there.
    ///
    /// Every step after the first is optional. A machine that is disconnected
    /// has no primary connection; a wired one has no access point. Those are
    /// answers, so the walk stops and reports what it has rather than failing.
    async fn read(&self) -> Result<Reading, BrokerError> {
        let mut reading = Reading {
            manager_state: self.property(&self.manager, "State").await.unwrap_or(0),
            ..Reading::default()
        };

        let Some(active) = self.path(&self.manager, "PrimaryConnection").await else {
            return Ok(reading);
        };
        let active = self.proxy(&active, Self::ACTIVE_IFACE).await?;

        reading.kind = self.property(&active, "Type").await.unwrap_or_default();
        reading.id = self.property(&active, "Id").await.unwrap_or_default();
        reading.is_vpn = self.property(&active, "Vpn").await.unwrap_or(false);

        let devices: Vec<OwnedObjectPath> =
            self.property(&active, "Devices").await.unwrap_or_default();
        let Some(device) = devices.first() else {
            return Ok(reading);
        };
        let device = self.proxy(device, Self::DEVICE_IFACE).await?;
        reading.interface = self
            .property(&device, "Interface")
            .await
            .unwrap_or_default();

        // Wireless only: a wired device has no access point and no strength,
        // and the type field is what says so.
        let wireless = self.proxy(device.path(), Self::WIRELESS_IFACE).await?;
        let Some(point) = self.path(&wireless, "ActiveAccessPoint").await else {
            return Ok(reading);
        };
        let point = self.proxy(&point, Self::AP_IFACE).await?;

        reading.strength = self.property(&point, "Strength").await.unwrap_or(0);
        // An SSID is bytes, not a string: it is whatever the network was named
        // with, which is not required to be UTF-8.
        let ssid: Vec<u8> = self.property(&point, "Ssid").await.unwrap_or_default();
        reading.ssid = String::from_utf8_lossy(&ssid).into_owned();

        Ok(reading)
    }

    async fn proxy(
        &self,
        path: &zbus::zvariant::ObjectPath<'_>,
        interface: &'static str,
    ) -> Result<Proxy<'static>, BrokerError> {
        Proxy::new(&self.connection, Self::SERVICE, path.to_owned(), interface)
            .await
            .map_err(Self::unreadable)
    }

    /// A property, or `None` where NetworkManager does not have it.
    ///
    /// Missing reads as absent rather than as a failure: the walk is a chain
    /// of optional steps, and a wired device that has no `Strength` has not
    /// gone wrong.
    async fn property<T>(&self, proxy: &Proxy<'_>, name: &str) -> Option<T>
    where
        T: TryFrom<zbus::zvariant::OwnedValue>,
        <T as TryFrom<zbus::zvariant::OwnedValue>>::Error: Into<zbus::Error>,
    {
        proxy.get_property(name).await.ok()
    }

    /// An object path property, or `None` for NetworkManager's "there is none".
    async fn path(&self, proxy: &Proxy<'_>, name: &str) -> Option<OwnedObjectPath> {
        let path: OwnedObjectPath = self.property(proxy, name).await?;
        match path.as_str() == Self::NONE {
            true => None,
            false => Some(path),
        }
    }

    fn unreadable(error: impl std::fmt::Display) -> BrokerError {
        BrokerError::Unreadable {
            subsystem: "network-manager",
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
pub struct NetworkManager {
    link: Option<Link>,
    tick: Cadence,
    primed: bool,
}

impl Default for NetworkManager {
    fn default() -> Self {
        Self::new()
    }
}

impl NetworkManager {
    /// The floor under the signals. Connecting and disconnecting arrive as
    /// `PropertiesChanged`; signal strength does not, because it lives on the
    /// access point and changes as somebody walks around.
    pub const REFRESH: Duration = Duration::from_secs(5);

    pub fn new() -> Self {
        Self {
            link: None,
            tick: Cadence::after(Self::REFRESH),
            primed: false,
        }
    }

    fn patch(state: NetworkState) -> StatePatch {
        StatePatch {
            topics: vec![StateTopic {
                topic: SystemTopic::Network.as_str().into(),
                revision: 0, // the Hub assigns the real revision
                value: Some(state_topic::Value::Network(state)),
            }],
        }
    }
}

#[async_trait]
impl Broker for NetworkManager {
    fn name(&self) -> &'static str {
        "network-manager"
    }

    fn topics(&self) -> &'static [SystemTopic] {
        &[SystemTopic::Network]
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
                // Both arms are cancel-safe: a signal stream is a receiver,
                // and an interval keeps its own deadline.
                tokio::select! {
                    change = link.changes.next() => change.is_none(),
                    _ = tick.wait() => false,
                }
            };
            if closed {
                self.link = None;
                self.primed = false;
                return Err(BrokerError::Unreadable {
                    subsystem: "network-manager",
                    detail: "the bus closed".into(),
                });
            }
        }

        let reading = self.link.as_ref().expect("opened above").read().await?;
        self.primed = true;
        Ok(Self::patch(reading.state()))
    }
}
