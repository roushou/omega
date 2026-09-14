//! Connectivity, Wi-Fi, tunnels, and interface traffic.

mod connectivity;
mod traffic;
mod vpn;
mod wifi;

pub use connectivity::Network;
pub use traffic::{Link, Throughput};
pub use vpn::{Tunnel, Vpn};
pub use wifi::{AccessPoint, Wifi, WifiControl};

pub use omega_proto::omega::WifiPhase;
