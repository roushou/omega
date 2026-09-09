//! Where the machine keeps things.

use crate::state::Disk;
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

    /// Where it is mounted: `/`, `/home`. Also its identity — a list keys
    /// rows by this, because it survives a device being renamed.
    pub fn path(&self) -> &str {
        &self.path
    }

    /// `/dev/nvme0n1p2`.
    pub fn device(&self) -> &str {
        &self.device
    }

    /// `ext4`, `btrfs`. Somebody else's vocabulary, so a string.
    pub fn filesystem(&self) -> &str {
        &self.filesystem
    }

    pub fn total(&self) -> Bytes {
        self.total
    }

    pub fn available(&self) -> Bytes {
        self.available
    }

    /// Total less available. Prints itself as `412.7 GiB`.
    pub fn used(&self) -> Bytes {
        self.total.less(self.available)
    }

    /// How full it is, or `None` for a filesystem that reported no size —
    /// which is a pseudo-filesystem, not a full one.
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
