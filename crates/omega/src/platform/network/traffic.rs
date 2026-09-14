//! Network interface traffic counters and rates.

crate::wiring::reading! {
    /// Traffic counters and rates for each network interface.
    Throughput: omega_proto::omega::ThroughputState
}

use crate::units::{Bytes, Rate};

/// Traffic counters and transfer rates for one network interface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    interface: String,
    rx: Rate,
    tx: Rate,
    rx_total: Bytes,
    tx_total: Bytes,
}

impl Link {
    fn of(link: omega_proto::omega::Link) -> Self {
        Self {
            interface: link.interface,
            rx: Rate::of(link.rx_bytes_per_sec),
            tx: Rate::of(link.tx_bytes_per_sec),
            rx_total: Bytes::of(link.rx_bytes_total),
            tx_total: Bytes::of(link.tx_bytes_total),
        }
    }

    /// Interface name, such as `wlan0` or `lo`. Suitable for item keys.
    pub fn interface(&self) -> &str {
        &self.interface
    }

    /// Incoming transfer rate in bytes per second.
    pub fn down(&self) -> Rate {
        self.rx
    }

    /// Outgoing transfer rate in bytes per second.
    pub fn up(&self) -> Rate {
        self.tx
    }

    /// Total bytes received since the interface counters were initialized.
    pub fn received(&self) -> Bytes {
        self.rx_total
    }

    pub fn sent(&self) -> Bytes {
        self.tx_total
    }

    /// Whether this is the loopback interface.
    pub fn is_loopback(&self) -> bool {
        self.interface == "lo"
    }
}

impl Throughput {
    /// Every interface the kernel lists, loopback included.
    pub fn links(&self) -> Vec<Link> {
        self.read()
            .map(|state| state.links.into_iter().map(Link::of).collect())
            .unwrap_or_default()
    }

    /// One interface by name.
    pub fn on(&self, interface: &str) -> Option<Link> {
        self.links()
            .into_iter()
            .find(|link| link.interface == interface)
    }

    /// Return aggregate incoming and outgoing rates, excluding loopback.
    pub fn total(&self) -> (Rate, Rate) {
        self.links().iter().filter(|link| !link.is_loopback()).fold(
            (Rate::ZERO, Rate::ZERO),
            |(down, up), link| {
                (
                    Rate::of(down.count() + link.down().count()),
                    Rate::of(up.count() + link.up().count()),
                )
            },
        )
    }
}
