//! The tunnels the machine is running through.

use crate::state::Vpn;

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

    /// The connection's name, as the person who made it typed it.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// `wg0`, `tun0`.
    pub fn interface(&self) -> &str {
        &self.interface
    }

    /// `wireguard`, `openvpn`, and whatever else NetworkManager grows. A
    /// string rather than an enum because the set is somebody else's.
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

    /// Whether anything is up. The question a bar indicator asks.
    pub fn is_connected(&self) -> bool {
        self.read().is_some_and(|state| !state.tunnels.is_empty())
    }
}
