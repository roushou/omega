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
use omega_proto::omega::{
    AccessPoint, NetworkState, NetworkType, StatePatch, StateTopic, Tunnel, VpnState, WifiState,
    state_topic,
};

use crate::broker::{Broker, BrokerError, Cadence, opaque_debug};
use crate::dbus;

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
        if self.ssid.is_empty() {
            self.id.clone()
        } else {
            self.ssid.clone()
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

/// One access point, as NetworkManager reports it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Point {
    pub ssid: String,
    /// 0..100.
    pub strength: u8,
    /// `NM80211ApFlags`. Bit 0 is privacy.
    pub flags: u32,
    /// `NM80211ApSecurityFlags`, WPA and RSN. Either being set is enough.
    pub wpa: u32,
    pub rsn: u32,
}

impl Point {
    /// `NM_802_11_AP_FLAGS_PRIVACY`.
    const PRIVACY: u32 = 0x1;

    fn secured(&self) -> bool {
        self.flags & Self::PRIVACY != 0 || self.wpa != 0 || self.rsn != 0
    }
}

/// What the last scan found, and what the machine is on.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Scan {
    pub points: Vec<Point>,
    /// The network the machine is associated with, so the list can say which.
    pub active: String,
}

impl Scan {
    /// The ontology's view: one entry per network, strongest first.
    ///
    /// A network is often several radios on the same SSID, and a picker
    /// listing each of them is showing the hardware rather than the choice —
    /// so they are folded, keeping the strongest, which is the one that would
    /// be joined anyway.
    ///
    /// Hidden networks broadcast an empty SSID. They are dropped: a row a
    /// user cannot tell from another row is not a choice.
    pub fn state(&self) -> WifiState {
        let mut best: Vec<AccessPoint> = Vec::new();

        for point in self.points.iter().filter(|point| !point.ssid.is_empty()) {
            match best.iter_mut().find(|held| held.ssid == point.ssid) {
                Some(held) => {
                    held.signal_percent = held.signal_percent.max(u32::from(point.strength));
                    held.secured |= point.secured();
                }
                None => best.push(AccessPoint {
                    ssid: point.ssid.clone(),
                    signal_percent: u32::from(point.strength),
                    secured: point.secured(),
                    active: point.ssid == self.active,
                }),
            }
        }

        // Strongest first, then by name so a list of equals does not reorder
        // itself under the cursor every scan.
        best.sort_by(|a, b| {
            b.signal_percent
                .cmp(&a.signal_percent)
                .then_with(|| a.ssid.cmp(&b.ssid))
        });
        WifiState {
            access_points: best,
        }
    }
}

/// One active connection that is a tunnel.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Active {
    pub id: String,
    /// The connection's `Type`: `"wireguard"`, `"vpn"`, `"802-11-wireless"`.
    pub kind: String,
    /// The `Vpn` flag, which NetworkManager sets for the plugins it drives
    /// and leaves false for kinds it handles natively.
    pub is_vpn: bool,
    pub interface: String,
}

impl Active {
    /// Whether this connection is a tunnel rather than the link under one.
    ///
    /// Two spellings, and neither alone is enough: NetworkManager sets the
    /// flag for its VPN plugins and leaves it false for WireGuard, which it
    /// drives natively.
    pub fn is_tunnel(&self) -> bool {
        self.is_vpn || matches!(self.kind.as_str(), "vpn" | "wireguard")
    }
}

/// The tunnels, turned into the ontology.
#[derive(Debug)]
pub struct Tunnels;

impl Tunnels {
    /// Every tunnel up, by name.
    ///
    /// A list because a machine can be on Wi-Fi *and* a VPN, and on two
    /// tunnels at once — which is unusual and not wrong. Sorted by name so a
    /// bar does not reorder them between readings.
    pub fn state(active: &[Active]) -> VpnState {
        let mut up: Vec<Tunnel> = active
            .iter()
            .filter(|connection| connection.is_tunnel())
            .map(|connection| Tunnel {
                name: connection.id.clone(),
                interface: connection.interface.clone(),
                kind: connection.kind.clone(),
            })
            .collect();

        up.sort_by(|a, b| a.name.cmp(&b.name));
        VpnState { tunnels: up }
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
        let connection = Connection::system()
            .await
            .map_err(BrokerError::unreadable)?;
        let manager = Proxy::new(
            &connection,
            Self::SERVICE,
            Self::MANAGER,
            Self::MANAGER_IFACE,
        )
        .await
        .map_err(BrokerError::unreadable)?;

        let properties = PropertiesProxy::builder(&connection)
            .destination(Self::SERVICE)
            .map_err(BrokerError::unreadable)?
            .path(Self::MANAGER)
            .map_err(BrokerError::unreadable)?
            .build()
            .await
            .map_err(BrokerError::unreadable)?;
        let changes = properties
            .receive_properties_changed()
            .await
            .map_err(BrokerError::unreadable)?;

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
            manager_state: dbus::property(&self.manager, "State").await.unwrap_or(0),
            ..Reading::default()
        };

        let Some(active) = self.path(&self.manager, "PrimaryConnection").await else {
            return Ok(reading);
        };
        let active = self.proxy(&active, Self::ACTIVE_IFACE).await?;

        reading.kind = dbus::property(&active, "Type").await.unwrap_or_default();
        reading.id = dbus::property(&active, "Id").await.unwrap_or_default();
        reading.is_vpn = dbus::property(&active, "Vpn").await.unwrap_or(false);

        let devices: Vec<OwnedObjectPath> =
            dbus::property(&active, "Devices").await.unwrap_or_default();
        let Some(device) = devices.first() else {
            return Ok(reading);
        };
        let device = self.proxy(device, Self::DEVICE_IFACE).await?;
        reading.interface = dbus::property(&device, "Interface")
            .await
            .unwrap_or_default();

        // Wireless only: a wired device has no access point and no strength,
        // and the type field is what says so.
        let wireless = self.proxy(device.path(), Self::WIRELESS_IFACE).await?;
        let Some(point) = self.path(&wireless, "ActiveAccessPoint").await else {
            return Ok(reading);
        };
        let point = self.proxy(&point, Self::AP_IFACE).await?;

        reading.strength = dbus::property(&point, "Strength").await.unwrap_or(0);
        // An SSID is bytes, not a string: it is whatever the network was named
        // with, which is not required to be UTF-8.
        let ssid: Vec<u8> = dbus::property(&point, "Ssid").await.unwrap_or_default();
        reading.ssid = String::from_utf8_lossy(&ssid).into_owned();

        Ok(reading)
    }

    /// Every access point the wireless device can see.
    ///
    /// From the device rather than the connection: a machine that is on
    /// nothing still scans, and a picker with no list is the case that most
    /// needs one.
    async fn scan(&self, active: String) -> Scan {
        let mut scan = Scan {
            active,
            ..Scan::default()
        };

        let Some(device) = self.wireless().await else {
            return scan;
        };
        let paths: Vec<OwnedObjectPath> = dbus::property(&device, "AccessPoints")
            .await
            .unwrap_or_default();

        for path in paths {
            let Ok(point) = self.proxy(&path, Self::AP_IFACE).await else {
                continue;
            };
            let ssid: Vec<u8> = dbus::property(&point, "Ssid").await.unwrap_or_default();
            scan.points.push(Point {
                ssid: String::from_utf8_lossy(&ssid).into_owned(),
                strength: dbus::property(&point, "Strength").await.unwrap_or(0),
                flags: dbus::property(&point, "Flags").await.unwrap_or(0),
                wpa: dbus::property(&point, "WpaFlags").await.unwrap_or(0),
                rsn: dbus::property(&point, "RsnFlags").await.unwrap_or(0),
            });
        }
        scan
    }

    /// Every active connection, with what it is and what it runs on.
    ///
    /// From `ActiveConnections` rather than `PrimaryConnection`: a VPN over
    /// Wi-Fi has both up at once, and the primary one is only ever the tunnel.
    async fn active(&self) -> Vec<Active> {
        let paths: Vec<OwnedObjectPath> = dbus::property(&self.manager, "ActiveConnections")
            .await
            .unwrap_or_default();

        let mut found = Vec::new();
        for path in paths {
            let Ok(connection) = self.proxy(&path, Self::ACTIVE_IFACE).await else {
                continue;
            };

            let devices: Vec<OwnedObjectPath> = dbus::property(&connection, "Devices")
                .await
                .unwrap_or_default();
            let interface = match devices.first() {
                Some(device) => match self.proxy(device, Self::DEVICE_IFACE).await {
                    Ok(device) => dbus::property(&device, "Interface")
                        .await
                        .unwrap_or_default(),
                    Err(_) => String::new(),
                },
                None => String::new(),
            };

            found.push(Active {
                id: dbus::property(&connection, "Id").await.unwrap_or_default(),
                kind: dbus::property(&connection, "Type")
                    .await
                    .unwrap_or_default(),
                is_vpn: dbus::property(&connection, "Vpn").await.unwrap_or(false),
                interface,
            });
        }
        found
    }

    /// The first wireless device, if the machine has one.
    async fn wireless(&self) -> Option<Proxy<'static>> {
        let devices: Vec<OwnedObjectPath> = dbus::property(&self.manager, "Devices").await?;
        for path in devices {
            let device = self.proxy(&path, Self::DEVICE_IFACE).await.ok()?;
            // `NM_DEVICE_TYPE_WIFI`.
            if dbus::property::<u32>(&device, "DeviceType").await == Some(2) {
                return self.proxy(&path, Self::WIRELESS_IFACE).await.ok();
            }
        }
        None
    }

    async fn proxy(
        &self,
        path: &zbus::zvariant::ObjectPath<'_>,
        interface: &'static str,
    ) -> Result<Proxy<'static>, BrokerError> {
        Proxy::new(&self.connection, Self::SERVICE, path.to_owned(), interface)
            .await
            .map_err(BrokerError::unreadable)
    }

    /// An object path property, or `None` for NetworkManager's "there is none".
    async fn path(&self, proxy: &Proxy<'_>, name: &str) -> Option<OwnedObjectPath> {
        let path: OwnedObjectPath = dbus::property(proxy, name).await?;
        if path.as_str() == Self::NONE {
            None
        } else {
            Some(path)
        }
    }
}

opaque_debug!(Link);

#[derive(Debug)]
pub struct NetworkManager {
    link: Option<Link>,
    tick: Cadence,
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
        }
    }

    /// Both topics in one patch. The hub coalesces per topic, so a signal
    /// that only moved the connection does not wake a picker, and a scan that
    /// only moved the list does not wake an indicator.
    fn patch(network: NetworkState, wifi: WifiState, vpn: VpnState) -> StatePatch {
        StatePatch {
            topics: vec![
                StateTopic {
                    topic: SystemTopic::Network.as_str().into(),
                    revision: 0, // the Hub assigns the real revision
                    value: Some(state_topic::Value::Network(network)),
                },
                StateTopic {
                    topic: SystemTopic::Wifi.as_str().into(),
                    revision: 0,
                    value: Some(state_topic::Value::Wifi(wifi)),
                },
                StateTopic {
                    topic: SystemTopic::Vpn.as_str().into(),
                    revision: 0,
                    value: Some(state_topic::Value::Vpn(vpn)),
                },
            ],
        }
    }
}

#[async_trait]
impl Broker for NetworkManager {
    fn name(&self) -> &'static str {
        "network-manager"
    }

    fn topics(&self) -> &'static [SystemTopic] {
        &[SystemTopic::Network, SystemTopic::Wifi, SystemTopic::Vpn]
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
        let reading = link.read().await?;
        let network = reading.state();
        let scan = link.scan(network.ssid.clone()).await;
        let active = link.active().await;
        Ok(Self::patch(network, scan.state(), Tunnels::state(&active)))
    }
}
