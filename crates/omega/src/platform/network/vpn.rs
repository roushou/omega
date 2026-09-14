//! Active VPN and tunnel connections.

crate::wiring::reading! {
    /// Active VPN and tunnel connections.
    Vpn: omega_proto::omega::VpnState
}

/// One tunnel, as NetworkManager reports it.
#[derive(Debug, Clone, PartialEq)]
pub struct Tunnel {
    name: String,
    interface: String,
    kind: String,
}

impl Tunnel {
    fn of(tunnel: omega_proto::omega::Tunnel) -> Self {
        Self {
            name: tunnel.name,
            interface: tunnel.interface,
            kind: tunnel.kind,
        }
    }

    /// Connection display name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Tunnel interface name, such as `wg0` or `tun0`.
    pub fn interface(&self) -> &str {
        &self.interface
    }

    /// NetworkManager connection type, such as `wireguard` or `openvpn`.
    pub fn kind(&self) -> &str {
        &self.kind
    }
}

impl Vpn {
    pub fn tunnels(&self) -> Vec<Tunnel> {
        self.read()
            .map(|state| state.tunnels.into_iter().map(Tunnel::of).collect())
            .unwrap_or_default()
    }

    /// Whether at least one tunnel is connected.
    pub fn is_connected(&self) -> bool {
        self.read().is_some_and(|state| !state.tunnels.is_empty())
    }
}
