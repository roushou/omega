//! How much is moving over the network.

use crate::reading::Throughput;
use crate::units::{Bytes, Rate};

/// One interface, and what is going over it right now.
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

    /// `wlan0`, `lo`. Its identity, so a list keys rows by it.
    pub fn interface(&self) -> &str {
        &self.interface
    }

    /// Coming in. Prints itself as `1.2 MiB/s`.
    pub fn down(&self) -> Rate {
        self.rx
    }

    /// Going out.
    pub fn up(&self) -> Rate {
        self.tx
    }

    /// Since boot, for a widget that wants a total rather than a rate.
    pub fn received(&self) -> Bytes {
        self.rx_total
    }

    pub fn sent(&self) -> Bytes {
        self.tx_total
    }

    /// The loopback, which is the machine talking to itself and is almost
    /// never what a bar means by "the network".
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

    /// Everything that is not the loopback, added up — what a bar slot draws
    /// when it just wants to say how busy the network is.
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
