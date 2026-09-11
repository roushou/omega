//! Network connectivity, Wi-Fi control, tunnels, and interface traffic.
//!
//! Holding a state handle subscribes the plugin to its topics. Control handles
//! belong on commands or reactions; a widget cannot hold them.
//!
//! ```
//! use omega::network::Network;
//!
//! #[derive(omega::Widget)]
//! struct Indicator {
//!     network: Network,
//! }
//! ```

pub use crate::effect::wifi::WifiControl;
pub use crate::reading::{AccessPoint, Link, Network, Throughput, Tunnel, Vpn, Wifi, WifiPhase};
