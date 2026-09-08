//! Battery state from sysfs.
//!
//! A stopgap until the UPower broker: sysfs reports a percentage and a
//! status string and nothing else, which is why `seconds_to_empty` and
//! `seconds_to_full` are zero here. The swap is local because a broker is
//! the only thing that holds its subsystem.

use std::path::PathBuf;
use std::time::Duration;

use async_trait::async_trait;

use omega_proto::SystemTopic;
use omega_proto::omega::{BatteryState, StatePatch, StateTopic, state_topic};

use crate::broker::{Broker, BrokerError, Cadence};

/// One battery's attribute directory under `/sys/class/power_supply`.
#[derive(Debug)]
struct Sysfs {
    dir: PathBuf,
}

impl Sysfs {
    const ROOT: &'static str = "/sys/class/power_supply";
    /// Point the broker at another tree — a fixture, or a machine whose
    /// power supplies live somewhere unusual.
    const ROOT_ENV: &'static str = "OMEGA_POWER_SUPPLY";

    fn root() -> PathBuf {
        std::env::var(Self::ROOT_ENV)
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from(Self::ROOT))
    }

    /// The first `BAT*` device, if the machine has one.
    fn discover() -> Option<Self> {
        let dir = std::fs::read_dir(Self::root())
            .ok()?
            .flatten()
            .find(|e| e.file_name().to_string_lossy().starts_with("BAT"))
            .map(|e| e.path())?;
        Some(Self { dir })
    }

    fn string(&self, attr: &str) -> Option<String> {
        std::fs::read_to_string(self.dir.join(attr))
            .ok()
            .map(|s| s.trim().to_string())
    }

    fn u32(&self, attr: &str) -> Option<u32> {
        self.string(attr)?.parse().ok()
    }

    fn state(&self) -> Option<BatteryState> {
        Some(BatteryState {
            level: self.u32("capacity")? as f64 / 100.0,
            charging: self.string("status")? == "Charging",
            seconds_to_empty: 0,
            seconds_to_full: 0,
        })
    }
}

#[derive(Debug)]
pub struct Battery {
    sysfs: Option<Sysfs>,
    tick: Cadence,
}

impl Default for Battery {
    fn default() -> Self {
        Self::new()
    }
}

impl Battery {
    /// How often sysfs is re-read. A battery moves slowly; a fixture driving
    /// one does not.
    pub const DEFAULT_INTERVAL: Duration = Duration::from_secs(2);

    pub fn new() -> Self {
        Self::every(Self::DEFAULT_INTERVAL)
    }

    pub fn every(interval: Duration) -> Self {
        Self {
            sysfs: None,
            tick: Cadence::every(interval),
        }
    }

    /// The current reading, rediscovering the device until one appears.
    fn read(&mut self) -> Option<BatteryState> {
        if self.sysfs.is_none() {
            self.sysfs = Sysfs::discover();
        }
        self.sysfs.as_ref()?.state()
    }

    /// One reading as a patch, with no cadence in the way.
    ///
    /// Separate from [`Broker::next`] because translating sysfs into the
    /// ontology and deciding when to do it are different jobs, and only the
    /// first is worth a test.
    ///
    /// The topic is always published, with no value when the machine has no
    /// battery. Saying nothing would be indistinguishable from not having
    /// been asked yet, and a unit that declared the topic waits for a first
    /// value before it draws — so a desktop's battery widget would hold its
    /// render forever rather than deciding it has nothing to show.
    pub fn patch(&mut self) -> StatePatch {
        StatePatch {
            topics: vec![StateTopic {
                topic: SystemTopic::Battery.as_str().into(),
                revision: 0, // the Hub assigns the real revision
                value: self.read().map(state_topic::Value::Battery),
            }],
        }
    }
}

#[async_trait]
impl Broker for Battery {
    fn name(&self) -> &'static str {
        "sysfs-battery"
    }

    fn topics(&self) -> &'static [SystemTopic] {
        &[SystemTopic::Battery]
    }

    async fn next(&mut self) -> Result<StatePatch, BrokerError> {
        self.tick.wait().await;
        Ok(self.patch())
    }
}
