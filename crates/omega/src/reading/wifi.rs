//! What the machine could connect to.

use crate::reading::Wifi;
use crate::units::Percent;

/// The networks on the air, as the last scan found them.
///
/// Separate from [`Network`], which is the one connection the machine *has*.
/// A picker holds this; an indicator holds that, and holding only what it
/// draws is what keeps an indicator from waking every time a signal jitters
/// three rooms away.
///
/// [`Network`]: crate::Network
impl Wifi {
    /// Every network, strongest first. One entry per name: a network is often
    /// several radios, and the daemon has already folded them.
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

    /// The one the machine is on, if it is on one in this list.
    pub fn active(&self) -> Option<AccessPoint> {
        self.networks().into_iter().find(AccessPoint::is_active)
    }
}

/// One network to choose from.
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

    /// Its name, which is also its identity: a list keys rows by this, and it
    /// is what a row hands back when it is chosen.
    pub fn ssid(&self) -> &str {
        &self.ssid
    }

    /// How well it is heard. Prints itself as `70%`.
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
    /// # fn example(wifi: &omega::reading::Wifi) {
    /// let connecting = wifi.phase() == omega::reading::WifiPhase::Connecting;
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
