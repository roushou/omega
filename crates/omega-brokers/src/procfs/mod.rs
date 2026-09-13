//! What the machine is doing with itself, from `/proc`.
//!
//! Four files, no dependency, and every one of them a fixed text format the
//! kernel has not changed in twenty years — so the whole translation is
//! testable against a captured string, and there is no connection to hold.
//!
//! Polled, because `/proc` has nothing to signal with. The interval is the
//! resolution: a system monitor that updated twice a second would be a system
//! monitor measuring itself.

use std::collections::BTreeMap;
use std::time::Duration;

use async_trait::async_trait;

use omega_proto::SystemTopic;
use omega_proto::omega::{Link, StatePatch, StateTopic, SystemState, ThroughputState, state_topic};

use crate::broker::{Broker, BrokerError, Cadence};

mod disks;
use disks::DiskSampler;
pub use disks::{Mounted, Mounts};

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
        if after.total < before.total || after.idle < before.idle {
            return 0;
        }
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

/// One interface's byte counters, as `/proc/net/dev` reports them.
///
/// Meaningless alone, like [`Jiffies`]: a rate is the change between two
/// samples over the time between them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Counters {
    pub rx: u64,
    pub tx: u64,
}

impl Counters {
    /// Bytes per second between two samples.
    ///
    /// Saturating, and zero when the counters went backwards — an interface
    /// that went away and came back starts from nought, and a widget should
    /// draw a gap rather than a spike of several gigabytes.
    pub fn between(before: Self, after: Self, seconds: u64) -> (u64, u64) {
        if seconds == 0 || after.rx < before.rx || after.tx < before.tx {
            return (0, 0);
        }
        (
            (after.rx - before.rx) / seconds,
            (after.tx - before.tx) / seconds,
        )
    }
}

/// The interface lines of `/proc/net/dev`, by name.
#[derive(Debug)]
pub struct Interfaces;

impl Interfaces {
    pub fn parse(dev: &str) -> BTreeMap<String, Counters> {
        dev.lines()
            // Two header lines, then `name: rx_bytes rx_packets … tx_bytes …`.
            .filter_map(|line| line.split_once(':'))
            .filter_map(|(name, rest)| {
                let fields: Vec<u64> = rest
                    .split_whitespace()
                    .map(|field| field.parse().unwrap_or(0))
                    .collect();
                Some((
                    name.trim().to_string(),
                    Counters {
                        rx: *fields.first()?,
                        // Receive has eight columns before transmit begins.
                        tx: *fields.get(8)?,
                    },
                ))
            })
            .collect()
    }
}

/// The CPU lines of `/proc/stat`. The first is the aggregate; the rest are
/// the cores, in order.
#[derive(Debug)]
pub struct Cpu;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum CpuId {
    Aggregate,
    Core(u32),
}

impl CpuId {
    fn parse(name: &str) -> Result<Self, BrokerError> {
        if name == "cpu" {
            return Ok(Self::Aggregate);
        }
        name.strip_prefix("cpu")
            .and_then(|id| id.parse().ok())
            .map(Self::Core)
            .ok_or_else(|| BrokerError::unreadable("invalid CPU identifier"))
    }
}

impl Cpu {
    pub fn parse(stat: &str) -> Result<Vec<Jiffies>, BrokerError> {
        Ok(Self::samples(stat)?.into_values().collect())
    }

    fn samples(stat: &str) -> Result<BTreeMap<CpuId, Jiffies>, BrokerError> {
        let mut samples = BTreeMap::new();
        for line in stat.lines().take_while(|line| line.starts_with("cpu")) {
            let mut fields = line.split_whitespace();
            let id = CpuId::parse(fields.next().unwrap_or_default())?;
            let counters = fields
                .map(str::parse::<u64>)
                .collect::<Result<Vec<_>, _>>()
                .map_err(BrokerError::unreadable)?;
            if counters.len() < 4 {
                return Err(BrokerError::unreadable("incomplete CPU counters"));
            }
            // Guest time is already included in user and nice time.
            let total = counters
                .iter()
                .take(8)
                .try_fold(0u64, |sum, value| sum.checked_add(*value))
                .ok_or_else(|| BrokerError::unreadable("CPU counters overflow"))?;
            let idle = counters[3].saturating_add(*counters.get(4).unwrap_or(&0));
            if samples.insert(id, Jiffies { total, idle }).is_some() {
                return Err(BrokerError::unreadable("duplicate CPU identifier"));
            }
        }
        if !samples.contains_key(&CpuId::Aggregate) {
            return Err(BrokerError::unreadable("missing aggregate CPU counters"));
        }
        Ok(samples)
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
    pub fn parse(meminfo: &str) -> Result<Self, BrokerError> {
        let mut memory = Self::default();
        let mut total_seen = false;
        let mut available_seen = false;
        for line in meminfo.lines() {
            let Some((name, rest)) = line.split_once(':') else {
                continue;
            };
            if !matches!(name, "MemTotal" | "MemAvailable" | "SwapTotal" | "SwapFree") {
                continue;
            }
            let kib = rest
                .split_whitespace()
                .next()
                .and_then(|v| v.parse::<u64>().ok())
                .ok_or_else(|| BrokerError::unreadable(format!("invalid {name}")))?;
            let bytes = kib
                .checked_mul(1024)
                .ok_or_else(|| BrokerError::unreadable(format!("{name} overflow")))?;
            match name {
                "MemTotal" => {
                    memory.total = bytes;
                    total_seen = true;
                }
                "MemAvailable" => {
                    memory.available = bytes;
                    available_seen = true;
                }
                "SwapTotal" => memory.swap_total = bytes,
                "SwapFree" => memory.swap_free = bytes,
                _ => {}
            }
        }
        if !total_seen
            || !available_seen
            || memory.total == 0
            || memory.available > memory.total
            || memory.swap_free > memory.swap_total
        {
            return Err(BrokerError::unreadable(
                "missing or invalid memory counters",
            ));
        }
        Ok(memory)
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

#[derive(Debug)]
pub struct Procfs {
    tick: Cadence,
    /// Utilisation requires two samples from the same CPU.
    last: Option<BTreeMap<CpuId, Jiffies>>,
    /// The last `/proc/net/dev` sample, for the same reason `last` exists:
    /// a rate is a change between two.
    links: BTreeMap<String, Counters>,
    disks: DiskSampler,
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

    pub fn new() -> Self {
        Self {
            tick: Cadence::after(Self::INTERVAL),
            last: None,
            links: BTreeMap::new(),
            disks: DiskSampler::default(),
            root: std::env::var(Self::ROOT_ENV)
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|_| std::path::PathBuf::from("/proc")),
        }
    }

    /// What is moving over each interface, per second.
    ///
    /// Zero on the first pass: there is nothing to subtract from yet, and a
    /// rate invented from a single counter would be everything since boot
    /// divided by two seconds.
    pub fn throughput(&mut self) -> ThroughputState {
        let now = Interfaces::parse(&self.file("net/dev"));
        let seconds = Self::INTERVAL.as_secs();

        let links = now
            .iter()
            .map(|(name, after)| {
                let before = self.links.get(name).copied().unwrap_or(*after);
                let (rx, tx) = Counters::between(before, *after, seconds);
                Link {
                    interface: name.clone(),
                    rx_bytes_per_sec: rx,
                    tx_bytes_per_sec: tx,
                    rx_bytes_total: after.rx,
                    tx_bytes_total: after.tx,
                }
            })
            .collect();

        self.links = now;
        ThroughputState { links }
    }

    /// The first successful sample establishes a baseline without publishing idle usage.
    pub fn reading(&mut self) -> Result<Option<SystemState>, BrokerError> {
        let samples = match self.system_sample() {
            Ok(sample) => sample,
            Err(error) => {
                self.last = None;
                return Err(error);
            }
        };
        let (samples, memory, uptime_seconds) = samples;
        let (load_1, load_5, load_15) = Load::parse(&self.file("loadavg"));
        let before = self.last.replace(samples.clone());
        let Some(before) = before else {
            return Ok(None);
        };
        let cpu_percent = Jiffies::between(before[&CpuId::Aggregate], samples[&CpuId::Aggregate]);
        let core_percent = samples
            .iter()
            .filter(|(id, _)| **id != CpuId::Aggregate)
            .map(|(id, after)| Jiffies::between(before.get(id).copied().unwrap_or(*after), *after))
            .collect();
        Ok(Some(SystemState {
            cpu_percent,
            core_percent,
            memory_total_bytes: memory.total,
            memory_available_bytes: memory.available,
            swap_total_bytes: memory.swap_total,
            swap_free_bytes: memory.swap_free,
            load_1,
            load_5,
            load_15,
            uptime_seconds,
        }))
    }

    fn system_sample(&self) -> Result<(BTreeMap<CpuId, Jiffies>, Memory, u64), BrokerError> {
        let samples = Cpu::samples(&std::fs::read_to_string(self.root.join("stat"))?)?;
        let memory = Memory::parse(&std::fs::read_to_string(self.root.join("meminfo"))?)?;
        let uptime = std::fs::read_to_string(self.root.join("uptime"))?;
        let seconds = uptime
            .split_whitespace()
            .next()
            .and_then(|value| value.parse::<f64>().ok())
            .filter(|value| value.is_finite() && *value >= 0.0)
            .ok_or_else(|| BrokerError::unreadable("invalid uptime"))?;
        Ok((samples, memory, seconds as u64))
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
        &[
            SystemTopic::System,
            SystemTopic::Disk,
            SystemTopic::Throughput,
        ]
    }

    async fn wake(&mut self) -> Result<(), BrokerError> {
        self.tick.wait().await;
        Ok(())
    }

    async fn read(&mut self) -> Result<StatePatch, BrokerError> {
        let mut topics = vec![StateTopic {
            topic: SystemTopic::Throughput.as_str().into(),
            revision: 0,
            value: Some(state_topic::Value::Throughput(self.throughput())),
        }];
        if let Some(system) = self.reading()? {
            topics.push(StateTopic {
                topic: SystemTopic::System.as_str().into(),
                revision: 0,
                value: Some(state_topic::Value::System(system)),
            });
        }
        match self.disks.poll(&self.root) {
            Ok(Some(disks)) => topics.push(StateTopic {
                topic: SystemTopic::Disk.as_str().into(),
                revision: 0,
                value: Some(state_topic::Value::Disk(disks)),
            }),
            Ok(None) => {}
            Err(error) => tracing::warn!(%error, "disk sampling failed"),
        }
        Ok(StatePatch { topics })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture {
        root: std::path::PathBuf,
    }
    impl Fixture {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!("omega-procfs-{}", std::process::id()));
            std::fs::create_dir_all(&root).unwrap();
            std::fs::write(
                root.join("meminfo"),
                "MemTotal: 100 kB\nMemAvailable: 50 kB",
            )
            .unwrap();
            std::fs::write(root.join("uptime"), "100.0 200.0").unwrap();
            Self { root }
        }
        fn stat(&self, value: &str) {
            std::fs::write(self.root.join("stat"), value).unwrap();
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.root).unwrap();
        }
    }

    #[test]
    fn hotplug_matches_cpu_identity_and_failed_reads_require_a_new_baseline() {
        let fixture = Fixture::new();
        let mut broker = Procfs::new();
        broker.root = fixture.root.clone();
        fixture.stat("cpu 100 0 0 900\ncpu0 50 0 0 450\ncpu2 50 0 0 450");
        assert!(broker.reading().unwrap().is_none());
        fixture.stat("cpu 150 0 0 950\ncpu1 900 0 0 100\ncpu2 75 0 0 475");
        let reading = broker.reading().unwrap().unwrap();
        assert_eq!(reading.cpu_percent, 50);
        assert_eq!(reading.core_percent, vec![0, 50]);
        std::fs::remove_file(fixture.root.join("meminfo")).unwrap();
        assert!(broker.reading().is_err());
        std::fs::write(
            fixture.root.join("meminfo"),
            "MemTotal: 100 kB\nMemAvailable: 0 kB",
        )
        .unwrap();
        assert!(broker.reading().unwrap().is_none());
        assert_eq!(broker.reading().unwrap().unwrap().memory_available_bytes, 0);
    }

    #[tokio::test(start_paused = true)]
    async fn the_second_sample_waits_a_full_interval() {
        let mut broker = Procfs::new();
        let started = tokio::time::Instant::now();
        broker.wake().await.unwrap();
        assert_eq!(tokio::time::Instant::now() - started, Procfs::INTERVAL);
    }
}
