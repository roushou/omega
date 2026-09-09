//! What the machine is doing with itself.

use crate::reading::System;
use crate::units::{Bytes, Percent, Uptime};

/// A pool of memory: how much there is, and how much of it is spoken for.
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

    /// Total less available, floored — the figure a panel actually shows.
    pub fn used(self) -> Bytes {
        self.total.less(self.available)
    }

    /// How full it is, or `None` on a machine with none of this kind — a
    /// desktop with no swap is not a desktop whose swap is full.
    pub fn share(self) -> Option<Percent> {
        self.used().share_of(self.total)
    }
}

/// How loaded the machine has been over the last one, five and fifteen
/// minutes, as the kernel reports them.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Load {
    pub one: f64,
    pub five: f64,
    pub fifteen: f64,
}

impl System {
    /// How busy the processor is, across every core. Prints itself as `12%`.
    pub fn cpu(&self) -> Percent {
        self.read()
            .map(|system| Percent::whole(system.cpu_percent.min(100) as u8))
            .unwrap_or(Percent::ZERO)
    }

    /// The same, per core, in the order the kernel lists them.
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

    /// How long since it booted. Prints itself as `3d 4h`.
    pub fn uptime(&self) -> Uptime {
        Uptime::seconds(self.read().map(|system| system.uptime_seconds).unwrap_or(0))
    }
}
