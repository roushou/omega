//! Wi-Fi scan results and connection control.

use crate::units::Percent;

use crate::runtime::context::Context;

use crate::wiring::does;

use omega_proto::omega::{ConnectWifi, DisconnectWifi, action};

crate::wiring::reading! {
    /// Wi-Fi scan results and connection status.
    Wifi: omega_proto::omega::WifiState
}

/// Wi-Fi scan results. Use [`Network`] for the primary connection status.
///
/// [`Network`]: crate::platform::network::Network
impl Wifi {
    /// Return scanned networks in descending signal-strength order.
    /// Access points with the same SSID are grouped into one entry.
    pub fn networks(&self) -> Vec<AccessPoint> {
        self.read()
            .map(|wifi| {
                wifi.access_points
                    .into_iter()
                    .map(AccessPoint::of)
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Return the active network if it appears in the current scan results.
    pub fn active(&self) -> Option<AccessPoint> {
        self.networks().into_iter().find(AccessPoint::is_active)
    }
}

/// A scanned Wi-Fi network.
#[derive(Debug, Clone, PartialEq)]
pub struct AccessPoint {
    ssid: String,
    strength: Percent,
    secured: bool,
    active: bool,
}

impl AccessPoint {
    fn of(point: omega_proto::omega::AccessPoint) -> Self {
        Self {
            ssid: point.ssid,
            strength: Percent::whole(point.signal_percent.min(100) as u8),
            secured: point.secured,
            active: point.active,
        }
    }

    /// Network SSID. Use it as the selection value and stable item key.
    pub fn ssid(&self) -> &str {
        &self.ssid
    }

    /// Signal strength as a percentage.
    pub fn strength(&self) -> Percent {
        self.strength
    }

    /// Whether joining it needs a passphrase.
    pub fn is_secured(&self) -> bool {
        self.secured
    }

    pub fn is_active(&self) -> bool {
        self.active
    }
}

impl Wifi {
    /// NetworkManager's reported connection phase; unavailable is unspecified.
    ///
    /// ```no_run
    /// # fn example(wifi: &omega::platform::network::Wifi) {
    /// let connecting = wifi.phase() == omega::platform::network::WifiPhase::Connecting;
    /// # }
    /// ```
    pub fn phase(&self) -> omega_proto::omega::WifiPhase {
        self.read()
            .and_then(|state| omega_proto::omega::WifiPhase::try_from(state.phase).ok())
            .unwrap_or_default()
    }
    /// The active or activating network, independent of the primary VPN route.
    pub fn ssid(&self) -> String {
        self.read().map(|state| state.ssid).unwrap_or_default()
    }
    /// The last failure reported by the current device state, if any.
    pub fn failure(&self) -> String {
        self.read().map(|state| state.failure).unwrap_or_default()
    }
}

/// Ask NetworkManager to connect or disconnect the wireless device.
/// Completion acknowledges the request; `Wifi` reports its outcome.
///
/// ```no_run
/// # async fn example(wifi: &omega::platform::network::WifiControl) -> omega::Result<()> {
/// wifi.connect("Home", "password").await?;
/// # Ok(()) }
/// ```
#[derive(Debug)]
pub struct WifiControl {
    context: Context,
}
does!(WifiControl, Network);
impl WifiControl {
    /// Connect to a visible open or personal Wi-Fi network. An empty password
    /// uses an existing saved profile for a secured network.
    pub fn connect(
        &self,
        ssid: impl Into<String>,
        password: impl Into<String>,
    ) -> crate::effect::Effect {
        self.act(action::Kind::ConnectWifi(ConnectWifi {
            ssid: ssid.into(),
            password: password.into(),
        }))
    }
    /// Disconnect Wi-Fi without disconnecting other devices.
    pub fn disconnect(&self) -> crate::effect::Effect {
        self.act(action::Kind::DisconnectWifi(DisconnectWifi {}))
    }
}
