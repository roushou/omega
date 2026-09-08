//! What the machine is doing with itself, from `/proc`.
//!
//! Four files, no dependency, and every one of them a fixed text format the
//! kernel has not changed in twenty years — so the whole translation is
//! testable against a captured string, and there is no connection to hold.
//!
//! Polled, because `/proc` has nothing to signal with. The interval is the
//! resolution: a system monitor that updated twice a second would be a system
//! monitor measuring itself.

use std::time::Duration;

use async_trait::async_trait;

use omega_proto::SystemTopic;
use omega_proto::omega::{DiskState, Mount, StatePatch, StateTopic, SystemState, state_topic};

use crate::broker::{Broker, BrokerError, Cadence};

/// One line of `/proc/stat`: how many jiffies a CPU spent in each state.
///
/// Meaningless alone. Utilisation is the *change* between two samples, which
/// is why a first reading reports nothing and the broker keeps the last one.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Jiffies {
    pub total: u64,
    pub idle: u64,
}

impl Jiffies {
    /// What fraction of the time between two samples was not idle.
    ///
    /// Whole percent: that is the resolution anything draws it at, and a
    /// value carrying decimals would be a new revision on every reading.
    pub fn between(before: Self, after: Self) -> u32 {
        let total = after.total.saturating_sub(before.total);
        let idle = after.idle.saturating_sub(before.idle);
        // No time passed, or the counters went backwards — a suspend, or a
        // core that came online. Zero is a better answer than a divide.
        if total == 0 || idle > total {
            return 0;
        }
        (((total - idle) as f64 / total as f64) * 100.0).round() as u32
    }
}

/// The CPU lines of `/proc/stat`. The first is the aggregate; the rest are
/// the cores, in order.
#[derive(Debug)]
pub struct Cpu;

impl Cpu {
    pub fn parse(stat: &str) -> Vec<Jiffies> {
        stat.lines()
            .take_while(|line| line.starts_with("cpu"))
            .filter_map(|line| {
                let fields: Vec<u64> = line
                    .split_whitespace()
                    .skip(1)
                    .filter_map(|field| field.parse().ok())
                    .collect();
                // user nice system idle iowait irq softirq steal …
                // Idle is idle plus iowait: a machine waiting on a disk is
                // not a machine doing work.
                let idle = fields.get(3)?.saturating_add(*fields.get(4).unwrap_or(&0));
                Some(Jiffies {
                    total: fields.iter().sum(),
                    idle,
                })
            })
            .collect()
    }
}

/// The fields of `/proc/meminfo` this reads, in bytes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Memory {
    pub total: u64,
    pub available: u64,
    pub swap_total: u64,
    pub swap_free: u64,
}

impl Memory {
    pub fn parse(meminfo: &str) -> Self {
        let mut memory = Self::default();
        for line in meminfo.lines() {
            let Some((name, rest)) = line.split_once(':') else {
                continue;
            };
            // Every value is in kibibytes, whatever the suffix says.
            let Some(kib) = rest
                .split_whitespace()
                .next()
                .and_then(|v| v.parse::<u64>().ok())
            else {
                continue;
            };
            let bytes = kib.saturating_mul(1024);
            match name {
                "MemTotal" => memory.total = bytes,
                "MemAvailable" => memory.available = bytes,
                "SwapTotal" => memory.swap_total = bytes,
                "SwapFree" => memory.swap_free = bytes,
                _ => {}
            }
        }
        memory
    }
}

/// `/proc/loadavg` and `/proc/uptime`, which are one number each and change.
#[derive(Debug)]
pub struct Load;

impl Load {
    /// The three averages. Absent or unparseable reads as zero rather than
    /// failing the whole reading — a load average is not worth losing the
    /// memory figures over.
    pub fn parse(loadavg: &str) -> (f64, f64, f64) {
        let mut fields = loadavg
            .split_whitespace()
            .map(|field| field.parse().unwrap_or(0.0));
        (
            fields.next().unwrap_or(0.0),
            fields.next().unwrap_or(0.0),
            fields.next().unwrap_or(0.0),
        )
    }

    /// Whole seconds. `/proc/uptime` carries hundredths, and a bar showing
    /// "up 3 days" does not.
    pub fn uptime(uptime: &str) -> u64 {
        uptime
            .split_whitespace()
            .next()
            .and_then(|field| field.parse::<f64>().ok())
            .map(|seconds| seconds as u64)
            .unwrap_or(0)
    }
}

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

#[derive(Debug)]
pub struct Procfs {
    tick: Cadence,
    /// The last `/proc/stat` sample. Utilisation is a change between two, so
    /// the first reading reports zero rather than a number it cannot know.
    last: Option<Vec<Jiffies>>,
    /// Ticks since the disks were last measured. A `statvfs` per mount is
    /// real work and free space moves in minutes, not seconds.
    since_disks: u32,
    root: std::path::PathBuf,
}

impl Default for Procfs {
    fn default() -> Self {
        Self::new()
    }
}

impl Procfs {
    /// How often the machine is measured. The interval *is* the resolution of
    /// the CPU figure, and a monitor updating twice a second would mostly be
    /// measuring itself.
    pub const INTERVAL: Duration = Duration::from_secs(2);

    /// Point the broker at another tree — a fixture, or a container whose
    /// `/proc` is somewhere unusual.
    const ROOT_ENV: &'static str = "OMEGA_PROC";

    /// One reading in this many is a disk reading. Free space moves in
    /// minutes; a `statvfs` per mount every two seconds would be this daemon
    /// spending more effort watching the disks than anything else does using
    /// them.
    pub const DISKS_EVERY: u32 = 15;

    pub fn new() -> Self {
        Self {
            tick: Cadence::every(Self::INTERVAL),
            last: None,
            since_disks: Self::DISKS_EVERY,
            root: std::env::var(Self::ROOT_ENV)
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|_| std::path::PathBuf::from("/proc")),
        }
    }

    /// One reading. A file that cannot be read leaves its fields at zero: a
    /// kernel without `/proc/loadavg` is unusual, and losing the memory
    /// figures over it would be worse.
    pub fn reading(&mut self) -> SystemState {
        let samples = Cpu::parse(&self.file("stat"));
        let memory = Memory::parse(&self.file("meminfo"));
        let (load_1, load_5, load_15) = Load::parse(&self.file("loadavg"));

        let percents = match self.last.take() {
            Some(before) => before
                .iter()
                .zip(samples.iter())
                .map(|(before, after)| Jiffies::between(*before, *after))
                .collect(),
            // Nothing to compare against yet.
            None => vec![0; samples.len()],
        };
        self.last = Some(samples);

        SystemState {
            cpu_percent: percents.first().copied().unwrap_or(0),
            core_percent: percents.into_iter().skip(1).collect(),
            memory_total_bytes: memory.total,
            memory_available_bytes: memory.available,
            swap_total_bytes: memory.swap_total,
            swap_free_bytes: memory.swap_free,
            load_1,
            load_5,
            load_15,
            uptime_seconds: Load::uptime(&self.file("uptime")),
        }
    }

    /// The disks, when it is their turn.
    ///
    /// `None` between turns, and a patch that carries no disk topic leaves
    /// the last one standing — which is what last-value-wins is for.
    pub fn disks(&mut self) -> Option<DiskState> {
        self.since_disks += 1;
        if self.since_disks < Self::DISKS_EVERY {
            return None;
        }
        self.since_disks = 0;

        let measured: Vec<Mount> = Mounts::parse(&self.file("mounts"))
            .into_iter()
            .filter_map(|mounted| Self::room(&mounted))
            .collect();

        Some(DiskState {
            mounts: Mounts::by_device(measured),
        })
    }

    /// How much room a mount has, or nothing where it cannot be asked — a
    /// stale network mount blocks rather than answers, and a disk list is not
    /// worth hanging the broker over.
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

    /// One file of `/proc`, or nothing where the kernel does not carry it.
    fn file(&self, file: &str) -> String {
        std::fs::read_to_string(self.root.join(file)).unwrap_or_default()
    }
}

#[async_trait]
impl Broker for Procfs {
    fn name(&self) -> &'static str {
        "procfs"
    }

    fn topics(&self) -> &'static [SystemTopic] {
        &[SystemTopic::System, SystemTopic::Disk]
    }

    async fn wake(&mut self) -> Result<(), BrokerError> {
        self.tick.wait().await;
        Ok(())
    }

    async fn read(&mut self) -> Result<StatePatch, BrokerError> {
        let mut topics = vec![StateTopic {
            topic: SystemTopic::System.as_str().into(),
            revision: 0, // the Hub assigns the real revision
            value: Some(state_topic::Value::System(self.reading())),
        }];

        if let Some(disks) = self.disks() {
            topics.push(StateTopic {
                topic: SystemTopic::Disk.as_str().into(),
                revision: 0,
                value: Some(state_topic::Value::Disk(disks)),
            });
        }

        Ok(StatePatch { topics })
    }
}
