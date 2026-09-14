//! Mounted filesystem capacity readings.

crate::wiring::reading! {
    /// Mounted filesystem capacity readings.
    Disk: omega_proto::omega::DiskState
}

use crate::units::{Bytes, Percent};

/// One mounted filesystem.
#[derive(Debug, Clone, PartialEq)]
pub struct Mount {
    path: String,
    device: String,
    filesystem: String,
    total: Bytes,
    available: Bytes,
}

impl Mount {
    fn of(mount: omega_proto::omega::Mount) -> Self {
        Self {
            path: mount.path,
            device: mount.device,
            filesystem: mount.filesystem,
            total: Bytes::of(mount.total_bytes),
            available: Bytes::of(mount.available_bytes),
        }
    }

    /// Mount path, such as `/` or `/home`. Suitable for item keys.
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Source device, such as `/dev/nvme0n1p2`.
    pub fn device(&self) -> &str {
        &self.device
    }

    /// Filesystem type reported by the system, such as `ext4` or `btrfs`.
    pub fn filesystem(&self) -> &str {
        &self.filesystem
    }

    pub fn total(&self) -> Bytes {
        self.total
    }

    pub fn available(&self) -> Bytes {
        self.available
    }

    /// Used space, computed as total minus available and saturated at zero.
    pub fn used(&self) -> Bytes {
        self.total.less(self.available)
    }

    /// Used fraction, or `None` if the reported total is zero.
    pub fn share(&self) -> Option<Percent> {
        self.used().share_of(self.total)
    }
}

impl Disk {
    pub fn mounts(&self) -> Vec<Mount> {
        self.read()
            .map(|state| state.mounts.into_iter().map(Mount::of).collect())
            .unwrap_or_default()
    }

    /// The filesystem mounted at a path, if it is one the broker reports.
    pub fn at(&self, path: &str) -> Option<Mount> {
        self.mounts().into_iter().find(|mount| mount.path == path)
    }
}
