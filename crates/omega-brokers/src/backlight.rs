//! Screen brightness from sysfs, in both directions.
//!
//! The kernel exposes a raw scale per device — 0..`max_brightness`, which is
//! 255 on one panel and 96000 on the next — and the ontology speaks percent.
//! Converting between them is this broker's whole job, and it is why reading
//! and writing belong together: a writer that rounded differently from the
//! reader would set 40% and report 39%.

use std::path::PathBuf;
use std::time::Duration;

use async_trait::async_trait;

use omega_proto::omega::{
    BacklightState, SetBacklight, StatePatch, StateTopic, action, set_backlight, state_topic,
};
use omega_proto::{ActionKind, SystemTopic};

use crate::broker::{Broker, BrokerError, Cadence};

/// One backlight device's attribute directory.
#[derive(Debug)]
struct Sysfs {
    dir: PathBuf,
}

impl Sysfs {
    const ROOT: &'static str = "/sys/class/backlight";
    /// Point the broker at another tree — a fixture, or a machine whose
    /// backlights live somewhere unusual.
    const ROOT_ENV: &'static str = "OMEGA_BACKLIGHT";

    fn root() -> PathBuf {
        std::env::var(Self::ROOT_ENV)
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from(Self::ROOT))
    }

    /// The first device, if the machine has one. A laptop has `intel_backlight`
    /// or `amdgpu_bl0`; a desktop with an external monitor has none, because
    /// DDC/CI is not a sysfs backlight.
    fn discover() -> Option<Self> {
        let dir = std::fs::read_dir(Self::root())
            .ok()?
            .flatten()
            .map(|entry| entry.path())
            .find(|path| path.join("max_brightness").is_file())?;
        Some(Self { dir })
    }

    fn u32(&self, attr: &str) -> Option<u32> {
        std::fs::read_to_string(self.dir.join(attr))
            .ok()?
            .trim()
            .parse()
            .ok()
    }

    /// The raw ceiling. Zero would make every percentage a division by zero,
    /// so a device reporting it is treated as no device at all.
    fn max(&self) -> Option<u32> {
        match self.u32("max_brightness")? {
            0 => None,
            max => Some(max),
        }
    }

    /// The setpoint — `brightness`, not `actual_brightness`.
    ///
    /// `actual_brightness` is the hardware's own answer, and it lags a write
    /// while the panel fades. Reporting it means a step that was just applied
    /// reads back as the old value, and the next relative step is then
    /// computed from a number the user has already moved away from. The
    /// setpoint is what was asked for, which is what arithmetic needs.
    fn raw(&self) -> Option<u32> {
        self.u32("brightness")
    }

    fn state(&self) -> Option<BacklightState> {
        let max = self.max()?;
        Some(BacklightState {
            percent: Backlight::to_percent(self.raw()?, max),
        })
    }

    fn write(&self, raw: u32) -> Result<(), BrokerError> {
        std::fs::write(self.dir.join("brightness"), raw.to_string())?;
        Ok(())
    }
}

#[derive(Debug)]
pub struct Backlight {
    sysfs: Option<Sysfs>,
    tick: Cadence,
}

impl Default for Backlight {
    fn default() -> Self {
        Self::new()
    }
}

impl Backlight {
    /// How often sysfs is re-read. Brightness changes when someone presses a
    /// key, and this broker publishes its own writes immediately, so the poll
    /// is only there to notice a change somebody else made.
    pub const DEFAULT_INTERVAL: Duration = Duration::from_secs(1);

    pub fn new() -> Self {
        Self::every(Self::DEFAULT_INTERVAL)
    }

    pub fn every(interval: Duration) -> Self {
        Self {
            sysfs: None,
            tick: Cadence::every(interval),
        }
    }

    /// Rounded to nearest, so 50% of a scale of 3 is 2 rather than 1.
    fn to_percent(raw: u32, max: u32) -> u32 {
        ((u64::from(raw) * 100 + u64::from(max) / 2) / u64::from(max)) as u32
    }

    fn to_raw(percent: u32, max: u32) -> u32 {
        let percent = percent.min(100);
        ((u64::from(percent) * u64::from(max) + 50) / 100) as u32
    }

    /// The device, rediscovering it until one appears.
    fn sysfs(&mut self) -> Option<&Sysfs> {
        if self.sysfs.is_none() {
            self.sysfs = Sysfs::discover();
        }
        self.sysfs.as_ref()
    }

    /// One reading as a patch, with no cadence in the way.
    ///
    /// The topic is always published, with no value on a machine that has no
    /// backlight — saying nothing would be indistinguishable from not having
    /// been asked yet, and a unit that declared the topic would wait forever.
    pub fn patch(&mut self) -> StatePatch {
        StatePatch {
            topics: vec![StateTopic {
                topic: SystemTopic::Backlight.as_str().into(),
                revision: 0, // the Hub assigns the real revision
                value: self
                    .sysfs()
                    .and_then(Sysfs::state)
                    .map(state_topic::Value::Backlight),
            }],
        }
    }

    /// Where a change lands, as a percentage.
    ///
    /// Pure, and separate from writing it, because the arithmetic is the part
    /// that can be wrong: a delta is applied to what the screen shows now,
    /// and both ends of the range are reachable.
    fn target(&self, change: &set_backlight::Change, current: u32) -> u32 {
        match change {
            set_backlight::Change::AbsolutePercent(percent) => (*percent).min(100),
            set_backlight::Change::DeltaPercent(delta) => {
                (current as i64 + i64::from(*delta)).clamp(0, 100) as u32
            }
        }
    }

    fn set(&mut self, set: &SetBacklight) -> Result<Option<StatePatch>, BrokerError> {
        let Some(change) = set.change.as_ref() else {
            return Err(BrokerError::Unreadable(
                "SetBacklight carries no change".into(),
            ));
        };

        let Some(sysfs) = self.sysfs() else {
            return Err(BrokerError::Unreadable(
                "this machine has no backlight".into(),
            ));
        };

        let max = sysfs.max().ok_or(BrokerError::Unreadable(
            "the device reports no scale".into(),
        ))?;
        let current = sysfs
            .raw()
            .map(|raw| Self::to_percent(raw, max))
            .unwrap_or(0);

        let percent = self.target(change, current);
        // Reborrowed: `target` reads `self`, and the write needs the device
        // again afterwards.
        let sysfs = self.sysfs().expect("discovered just above");
        sysfs.write(Self::to_raw(percent, max))?;

        // The broker that just set it knows the new value. Waiting a second
        // for the poll to notice is a slider that lags its own drag.
        Ok(Some(self.patch()))
    }
}

#[async_trait]
impl Broker for Backlight {
    fn name(&self) -> &'static str {
        "sysfs-backlight"
    }

    fn topics(&self) -> &'static [SystemTopic] {
        &[SystemTopic::Backlight]
    }

    fn actions(&self) -> &'static [ActionKind] {
        &[ActionKind::SetBacklight]
    }

    async fn wake(&mut self) -> Result<(), BrokerError> {
        self.tick.wait().await;
        Ok(())
    }

    async fn read(&mut self) -> Result<StatePatch, BrokerError> {
        Ok(self.patch())
    }

    async fn act(&mut self, action: &action::Kind) -> Result<Option<StatePatch>, BrokerError> {
        match action {
            action::Kind::SetBacklight(set) => self.set(set),
            other => Err(BrokerError::Unserved(ActionKind::of(other))),
        }
    }
}
