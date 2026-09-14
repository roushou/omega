//! CPU, memory, and uptime readings.

crate::wiring::reading! {
    /// CPU, memory, and uptime readings.
    System: omega_proto::omega::SystemState
}

use crate::units::{Bytes, Percent, Uptime};

/// Total and available memory for a memory pool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Memory {
    total: Bytes,
    available: Bytes,
}

impl Memory {
    pub fn total(self) -> Bytes {
        self.total
    }

    pub fn available(self) -> Bytes {
        self.available
    }

    /// Used bytes, computed as total minus available and saturated at zero.
    pub fn used(self) -> Bytes {
        self.total.less(self.available)
    }

    /// Used fraction, or `None` if the total is zero.
    pub fn share(self) -> Option<Percent> {
        self.used().share_of(self.total)
    }
}

/// CPU load averages over one, five, and fifteen minutes.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Load {
    pub one: f64,
    pub five: f64,
    pub fifteen: f64,
}

impl System {
    /// Aggregate CPU utilization across all cores.
    pub fn cpu(&self) -> Percent {
        self.read()
            .map(|system| Percent::whole(system.cpu_percent.min(100) as u8))
            .unwrap_or(Percent::ZERO)
    }

    /// Per-core CPU utilization in kernel enumeration order.
    pub fn cores(&self) -> Vec<Percent> {
        self.read()
            .map(|system| {
                system
                    .core_percent
                    .into_iter()
                    .map(|core| Percent::whole(core.min(100) as u8))
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn memory(&self) -> Memory {
        self.read()
            .map(|system| Memory {
                total: Bytes::of(system.memory_total_bytes),
                available: Bytes::of(system.memory_available_bytes),
            })
            .unwrap_or_default()
    }

    pub fn swap(&self) -> Memory {
        self.read()
            .map(|system| Memory {
                total: Bytes::of(system.swap_total_bytes),
                available: Bytes::of(system.swap_free_bytes),
            })
            .unwrap_or_default()
    }

    pub fn load(&self) -> Load {
        self.read()
            .map(|system| Load {
                one: system.load_1,
                five: system.load_5,
                fifteen: system.load_15,
            })
            .unwrap_or_default()
    }

    /// Time elapsed since system boot.
    pub fn uptime(&self) -> Uptime {
        Uptime::seconds(self.read().map(|system| system.uptime_seconds).unwrap_or(0))
    }
}
