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
use omega_proto::omega::{StatePatch, StateTopic, SystemState, state_topic};

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

#[derive(Debug)]
pub struct Procfs {
    tick: Cadence,
    /// The last `/proc/stat` sample. Utilisation is a change between two, so
    /// the first reading reports zero rather than a number it cannot know.
    last: Option<Vec<Jiffies>>,
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
            tick: Cadence::every(Self::INTERVAL),
            last: None,
            root: std::env::var(Self::ROOT_ENV)
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|_| std::path::PathBuf::from("/proc")),
        }
    }

    /// One reading. A file that cannot be read leaves its fields at zero: a
    /// kernel without `/proc/loadavg` is unusual, and losing the memory
    /// figures over it would be worse.
    pub fn reading(&mut self) -> SystemState {
        let samples = Cpu::parse(&self.read("stat"));
        let memory = Memory::parse(&self.read("meminfo"));
        let (load_1, load_5, load_15) = Load::parse(&self.read("loadavg"));

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
            uptime_seconds: Load::uptime(&self.read("uptime")),
        }
    }

    fn read(&self, file: &str) -> String {
        std::fs::read_to_string(self.root.join(file)).unwrap_or_default()
    }
}

#[async_trait]
impl Broker for Procfs {
    fn name(&self) -> &'static str {
        "procfs"
    }

    fn topics(&self) -> &'static [SystemTopic] {
        &[SystemTopic::System]
    }

    async fn next(&mut self) -> Result<StatePatch, BrokerError> {
        self.tick.wait().await;
        Ok(StatePatch {
            topics: vec![StateTopic {
                topic: SystemTopic::System.as_str().into(),
                revision: 0, // the Hub assigns the real revision
                value: Some(state_topic::Value::System(self.reading())),
            }],
        })
    }
}
