//! Filesystem queries isolated from the system sampling cadence.

use crate::broker::BrokerError;
use omega_proto::omega::{DiskState, Mount};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::Duration;
use tokio::time::Instant;

/// One line of `/proc/mounts`, before anything is asked of the filesystem.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Mounted {
    pub device: String,
    pub path: String,
    pub filesystem: String,
}

/// Which mounts are worth reporting.
#[derive(Debug)]
pub struct Mounts;

impl Mounts {
    /// Filesystems that are the kernel talking to itself.
    ///
    /// A machine has forty-odd mounts and four of them are disks. A bar
    /// listing `cgroup2` and eleven `tmpfs` is listing the kernel's furniture,
    /// and the one the user cares about is somewhere in the middle of it.
    ///
    /// A fast path, not the rule. This list will never be complete — there is
    /// always another pseudo-filesystem — so what actually decides is whether
    /// a mount reports any blocks at all, which is asked after.
    const PSEUDO: &'static [&'static str] = &[
        "autofs",
        "bpf",
        "cgroup",
        "cgroup2",
        "configfs",
        "debugfs",
        "devpts",
        "devtmpfs",
        "efivarfs",
        "fuse.gvfsd-fuse",
        "fuse.portal",
        "fusectl",
        "hugetlbfs",
        "mqueue",
        "proc",
        "pstore",
        "ramfs",
        "securityfs",
        "sysfs",
        "tmpfs",
        "tracefs",
    ];

    /// One entry per device, at its shortest mount point.
    ///
    /// Subvolumes and bind mounts put one filesystem at several paths — a
    /// btrfs root is often also `/home` and `/var/log` — and they share the
    /// space, so reporting each is reporting the same disk three times with
    /// the same numbers. The shortest path is the one a person means.
    pub fn by_device(mut measured: Vec<Mount>) -> Vec<Mount> {
        measured.sort_by(|a, b| {
            a.device
                .cmp(&b.device)
                .then_with(|| a.path.len().cmp(&b.path.len()))
                .then_with(|| a.path.cmp(&b.path))
        });
        measured.dedup_by(|a, b| a.device == b.device);

        // Back into the order a person reads: by where it is mounted.
        measured.sort_by(|a, b| a.path.cmp(&b.path));
        measured
    }

    pub fn parse(mounts: &str) -> Vec<Mounted> {
        mounts
            .lines()
            .filter_map(|line| {
                let mut fields = line.split_whitespace();
                let device = fields.next()?;
                let path = fields.next()?;
                let filesystem = fields.next()?;
                Some(Mounted {
                    device: device.to_string(),
                    // `/proc/mounts` escapes a space in a path as `\040`, and
                    // a mount point with one is a mount point, not two.
                    path: path.replace("\\040", " "),
                    filesystem: filesystem.to_string(),
                })
            })
            .filter(|mounted| !Self::PSEUDO.contains(&mounted.filesystem.as_str()))
            .collect()
    }
}

#[derive(Debug, Default)]
pub(super) struct DiskSampler {
    pending: Option<Receiver<Result<DiskState, BrokerError>>>,
    next: Option<Instant>,
}

impl DiskSampler {
    const INTERVAL: Duration = Duration::from_secs(30);

    pub(super) fn poll(&mut self, root: &Path) -> Result<Option<DiskState>, BrokerError> {
        if let Some(pending) = &self.pending {
            match pending.try_recv() {
                Ok(reading) => {
                    self.pending = None;
                    return reading.map(Some);
                }
                Err(TryRecvError::Empty) => return Ok(None),
                Err(TryRecvError::Disconnected) => {
                    self.pending = None;
                    return Err(BrokerError::unreadable("disk worker disconnected"));
                }
            }
        }
        if self.next.is_some_and(|next| Instant::now() < next) {
            return Ok(None);
        }
        let (send, receive) = mpsc::channel();
        let root = root.to_path_buf();
        // A blocked statvfs cannot be cancelled. Keep its receiver until completion
        // so retries cannot accumulate threads; a plain thread does not hold runtime shutdown.
        std::thread::Builder::new()
            .name("omega-disks".into())
            .spawn(move || {
                let _ = send.send(Self::measure(root));
            })?;
        self.pending = Some(receive);
        self.next = Some(Instant::now() + Self::INTERVAL);
        Ok(None)
    }

    fn measure(root: PathBuf) -> Result<DiskState, BrokerError> {
        let mounts = std::fs::read_to_string(root.join("mounts"))?;
        let measured = Mounts::parse(&mounts)
            .iter()
            .filter_map(Self::room)
            .collect();
        Ok(DiskState {
            mounts: Mounts::by_device(measured),
        })
    }

    /// Filesystem queries run on the single disk worker, never the async runtime.
    fn room(mounted: &Mounted) -> Option<Mount> {
        let path = std::ffi::CString::new(mounted.path.as_bytes()).ok()?;
        // SAFETY: `stats` is written by `statvfs` before it is read, and the
        // path is a NUL-terminated C string that outlives the call.
        let stats = unsafe {
            let mut stats: libc::statvfs = std::mem::zeroed();
            match libc::statvfs(path.as_ptr(), &mut stats) {
                0 => stats,
                _ => return None,
            }
        };

        let block = stats.f_frsize as u64;
        let total = (stats.f_blocks as u64).saturating_mul(block);

        // What actually separates a disk from the kernel's furniture: a
        // filesystem with no blocks is not somewhere anything is kept. Catches
        // every pseudo-filesystem the name list above does not know about.
        if total == 0 {
            return None;
        }

        Some(Mount {
            path: mounted.path.clone(),
            device: mounted.device.clone(),
            filesystem: mounted.filesystem.clone(),
            total_bytes: total,
            // `f_bavail`, not `f_bfree`: the difference is the blocks reserved
            // for root, and counting those as free is how a disk looks like it
            // has room right up until nothing can be saved.
            available_bytes: (stats.f_bavail as u64).saturating_mul(block),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(start_paused = true)]
    async fn a_stalled_query_stays_single_and_completion_respects_cadence() {
        let (send, receive) = mpsc::channel();
        let mut sampler = DiskSampler {
            pending: Some(receive),
            next: Some(Instant::now() + DiskSampler::INTERVAL),
        };
        for _ in 0..3 {
            assert!(sampler.poll(Path::new("/nonexistent")).unwrap().is_none());
        }
        send.send(Ok(DiskState::default())).unwrap();
        assert!(sampler.poll(Path::new("/nonexistent")).unwrap().is_some());
        assert!(sampler.poll(Path::new("/nonexistent")).unwrap().is_none());
        assert!(sampler.pending.is_none());
        tokio::time::advance(Duration::from_secs(300)).await;
        let (send, receive) = mpsc::channel();
        sampler.pending = Some(receive);
        for _ in 0..3 {
            assert!(sampler.poll(Path::new("/nonexistent")).unwrap().is_none());
        }
        send.send(Err(BrokerError::unreadable("fixture error")))
            .unwrap();
        assert!(sampler.poll(Path::new("/nonexistent")).is_err());
        assert!(sampler.pending.is_none());
    }

    #[test]
    fn disk_measurement_reports_real_capacity_and_file_failures() {
        let disks = DiskSampler::measure("/proc".into()).unwrap();
        assert!(
            disks
                .mounts
                .iter()
                .any(|mount| mount.path == "/" && mount.total_bytes > 0)
        );
        assert!(DiskSampler::measure("/nonexistent".into()).is_err());
    }
}
