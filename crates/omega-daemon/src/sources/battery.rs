//! Battery state from sysfs. A stopgap until the UPower source; the swap is
//! local because sources are just [`StateSource`] impls.

use std::path::PathBuf;
use std::time::Duration;

use async_trait::async_trait;

use omega_proto::omega::{BatteryState, StatePatch, StateTopic, state_topic};

use crate::source::StateSource;

/// One battery's attribute directory under `/sys/class/power_supply`.
#[derive(Debug)]
struct Sysfs {
    dir: PathBuf,
}

impl Sysfs {
    const ROOT: &'static str = "/sys/class/power_supply";
    /// Point the source at another tree — a fixture, or a machine whose
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
    interval: Duration,
}

impl Default for Battery {
    fn default() -> Self {
        Self {
            sysfs: None,
            interval: Duration::from_secs(2),
        }
    }
}

impl Battery {
    pub fn new() -> Self {
        Self::default()
    }

    /// The current reading, rediscovering the device until one appears.
    fn read(&mut self) -> Option<BatteryState> {
        if self.sysfs.is_none() {
            self.sysfs = Sysfs::discover();
        }
        self.sysfs.as_ref()?.state()
    }

    /// Poll faster than the default: a battery changes slowly, but a fixture
    /// driving one does not.
    pub fn every(interval: Duration) -> Self {
        Self {
            interval,
            ..Self::default()
        }
    }
}

#[async_trait]
impl StateSource for Battery {
    type Error = std::io::Error;

    fn name(&self) -> &'static str {
        "battery"
    }

    fn interval(&self) -> Duration {
        self.interval
    }

    async fn poll(&mut self) -> Result<StatePatch, Self::Error> {
        let topics = match self.read() {
            Some(state) => vec![StateTopic {
                topic: "battery".into(),
                revision: 0, // the Hub assigns the real revision
                value: Some(state_topic::Value::Battery(state)),
            }],
            None => Vec::new(),
        };
        Ok(StatePatch { topics })
    }
}
