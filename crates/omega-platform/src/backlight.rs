//! Read sysfs display backlights and write through logind.
//! Convert raw device values to percentages using the same rounding for reads and writes.

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

    /// Select the first sysfs backlight device, if present. External DDC/CI monitors are unsupported.
    fn discover(root: &std::path::Path) -> Option<Self> {
        let dir = std::fs::read_dir(root)
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

    /// Read the brightness setpoint. Hardware `actual_brightness` may lag writes
    /// and must not be used as the base for successive relative adjustments.
    fn raw(&self) -> Option<u32> {
        self.u32("brightness")
    }

    fn state(&self) -> Option<BacklightState> {
        let max = self.max()?;
        Some(BacklightState {
            percent: Backlight::to_percent(self.raw()?, max),
        })
    }

    fn needs_logind(&self, error: &std::io::Error) -> bool {
        // Fixture roots must never redirect a write to real hardware.
        self.dir.parent() == Some(std::path::Path::new(Self::ROOT))
            && error.kind() == std::io::ErrorKind::PermissionDenied
    }

    async fn write(&self, raw: u32) -> Result<(), BrokerError> {
        match std::fs::write(self.dir.join("brightness"), raw.to_string()) {
            Ok(()) => Ok(()),
            Err(error) if self.needs_logind(&error) => {
                let device = self
                    .dir
                    .file_name()
                    .and_then(|name| name.to_str())
                    .ok_or_else(|| {
                        BrokerError::Unreadable("invalid backlight device name".into())
                    })?;
                tokio::time::timeout(Duration::from_secs(3), LogindBacklight::set(device, raw))
                    .await
                    .map_err(|_| {
                        BrokerError::Unreadable("logind brightness request timed out".into())
                    })?
            }
            Err(error) => Err(error.into()),
        }
    }
}

struct LogindBacklight;
impl LogindBacklight {
    async fn set(device: &str, raw: u32) -> Result<(), BrokerError> {
        let connection = zbus::Connection::system()
            .await
            .map_err(BrokerError::unreadable)?;
        // `auto` also resolves the user's session for a systemd user service.
        let session = zbus::Proxy::new(
            &connection,
            "org.freedesktop.login1",
            "/org/freedesktop/login1/session/auto",
            "org.freedesktop.login1.Session",
        )
        .await
        .map_err(BrokerError::unreadable)?;
        session
            .call::<_, _, ()>("SetBrightness", &("backlight", device, raw))
            .await
            .map_err(BrokerError::unreadable)
    }
}

#[derive(Debug)]
pub struct Backlight {
    root: PathBuf,
    sysfs: Option<Sysfs>,
    tick: Cadence,
}

impl Default for Backlight {
    fn default() -> Self {
        Self::new()
    }
}

impl Backlight {
    /// Poll for external changes; broker writes publish their new values immediately.
    pub const DEFAULT_INTERVAL: Duration = Duration::from_secs(1);

    pub fn new() -> Self {
        Self::every(Self::DEFAULT_INTERVAL)
    }

    pub fn every(interval: Duration) -> Self {
        Self {
            root: Sysfs::root(),
            sysfs: None,
            tick: Cadence::every(interval),
        }
    }

    /// Discover devices under an explicit sysfs root.
    pub fn at(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            sysfs: None,
            tick: Cadence::every(Self::DEFAULT_INTERVAL),
        }
    }

    /// Round to nearest raw device step.
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
            self.sysfs = Sysfs::discover(&self.root);
        }
        self.sysfs.as_ref()
    }

    /// Publish a backlight reading or explicit absence when no device exists.
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

    /// Compute and clamp an absolute or relative percentage change.
    fn target(&self, change: &set_backlight::Change, current: u32) -> u32 {
        match change {
            set_backlight::Change::AbsolutePercent(percent) => (*percent).min(100),
            set_backlight::Change::DeltaPercent(delta) => {
                (current as i64 + i64::from(*delta)).clamp(0, 100) as u32
            }
        }
    }

    async fn set(&mut self, set: &SetBacklight) -> Result<Option<StatePatch>, BrokerError> {
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
        sysfs.write(Self::to_raw(percent, max)).await?;

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
            action::Kind::SetBacklight(set) => self.set(set).await,
            other => Err(BrokerError::Unserved(ActionKind::of(other))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_permission_failures_on_real_backlights_use_logind() {
        let real = Sysfs {
            dir: PathBuf::from("/sys/class/backlight/amdgpu_bl1"),
        };
        let fixture = Sysfs {
            dir: PathBuf::from("/tmp/backlight/amdgpu_bl1"),
        };
        let denied = std::io::Error::from(std::io::ErrorKind::PermissionDenied);
        assert!(real.needs_logind(&denied));
        assert!(!fixture.needs_logind(&denied));
        assert!(!real.needs_logind(&std::io::Error::from(std::io::ErrorKind::NotFound)));
    }
}
