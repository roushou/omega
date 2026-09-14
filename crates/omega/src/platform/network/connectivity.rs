//! Primary network connection state.

crate::wiring::reading! {
    /// Primary network connection state.
    Network: omega_proto::omega::NetworkState
}

use crate::units::Percent;

/// The machine's network connection.
impl Network {
    pub fn is_connected(&self) -> bool {
        self.read().is_some_and(|network| network.connected)
    }

    /// The network's name, or `None` when it is wired or disconnected.
    pub fn ssid(&self) -> Option<String> {
        self.read()
            .map(|network| network.ssid)
            .filter(|ssid| !ssid.is_empty())
    }

    /// Wireless signal strength as a percentage.
    pub fn strength(&self) -> Percent {
        self.read()
            .map(|network| Percent::whole(network.signal_percent.min(100) as u8))
            .unwrap_or(Percent::ZERO)
    }
}
