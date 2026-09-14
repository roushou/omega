//! Read temperature and fan sensors from `/sys/class/hwmon`.
//! Skip unreadable sensors; retain numeric zero as a valid measurement.

use std::path::{Path, PathBuf};
use std::time::Duration;

use async_trait::async_trait;

use omega_proto::SystemTopic;
use omega_proto::omega::{Fan, Sensor, StatePatch, StateTopic, ThermalsState, state_topic};

use crate::broker::{Broker, BrokerError, Cadence};

#[derive(Debug)]
pub struct Hwmon {
    tick: Cadence,
    root: PathBuf,
}

impl Default for Hwmon {
    fn default() -> Self {
        Self::new()
    }
}

impl Hwmon {
    /// Thermal polling interval.
    pub const INTERVAL: Duration = Duration::from_secs(5);

    /// Point the broker at another tree — a fixture, or a container.
    const ROOT_ENV: &'static str = "OMEGA_HWMON";

    pub fn new() -> Self {
        let root = std::env::var(Self::ROOT_ENV)
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("/sys/class/hwmon"));
        Self::at(&root)
    }

    /// The same, pointed at a given tree. What a test builds a fixture for.
    pub fn at(root: &Path) -> Self {
        Self {
            tick: Cadence::every(Self::INTERVAL),
            root: root.to_path_buf(),
        }
    }

    /// Every chip's directory, sorted so a list does not reorder between
    /// readings for no reason.
    fn chips(&self) -> Vec<PathBuf> {
        let Ok(entries) = std::fs::read_dir(&self.root) else {
            return Vec::new();
        };
        let mut chips: Vec<PathBuf> = entries
            .filter_map(|entry| entry.ok().map(|entry| entry.path()))
            .collect();
        chips.sort();
        chips
    }

    /// Read each chip independently; skip inaccessible chips.
    pub fn reading(&self) -> ThermalsState {
        let mut sensors = Vec::new();
        let mut fans = Vec::new();

        for chip in self.chips() {
            let name = Self::text(&chip.join("name")).unwrap_or_else(|| {
                chip.file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default()
            });

            for index in 1..=Self::MOST {
                if let Some(millicelsius) = Self::number::<i32>(&chip, "temp", index, "input") {
                    sensors.push(Sensor {
                        chip: name.clone(),
                        label: Self::label(&chip, "temp", index),
                        millicelsius,
                    });
                }
                if let Some(rpm) = Self::number::<u32>(&chip, "fan", index, "input") {
                    fans.push(Fan {
                        chip: name.clone(),
                        label: Self::label(&chip, "fan", index),
                        rpm,
                    });
                }
            }
        }

        ThermalsState { sensors, fans }
    }

    /// Bound the number of sensor files probed per chip.
    const MOST: u32 = 16;

    /// A sensor's label, or its name in the chip where it has none — `temp3`
    /// is a worse thing to draw than `CPU` and a better thing than nothing.
    fn label(chip: &Path, kind: &str, index: u32) -> String {
        Self::text(&chip.join(format!("{kind}{index}_label")))
            .unwrap_or_else(|| format!("{kind}{index}"))
    }

    fn number<T: std::str::FromStr>(chip: &Path, kind: &str, index: u32, field: &str) -> Option<T> {
        Self::text(&chip.join(format!("{kind}{index}_{field}")))?
            .parse()
            .ok()
    }

    /// One sysfs file, trimmed. `None` for anything that will not read —
    /// which for a disabled sensor is the normal case, not a fault.
    fn text(path: &Path) -> Option<String> {
        std::fs::read_to_string(path)
            .ok()
            .map(|text| text.trim().to_string())
            .filter(|text| !text.is_empty())
    }
}

#[async_trait]
impl Broker for Hwmon {
    fn name(&self) -> &'static str {
        "hwmon"
    }

    fn topics(&self) -> &'static [SystemTopic] {
        &[SystemTopic::Thermals]
    }

    async fn wake(&mut self) -> Result<(), BrokerError> {
        self.tick.wait().await;
        Ok(())
    }

    async fn read(&mut self) -> Result<StatePatch, BrokerError> {
        Ok(StatePatch {
            topics: vec![StateTopic {
                topic: SystemTopic::Thermals.as_str().into(),
                revision: 0, // the Hub assigns the real revision
                value: Some(state_topic::Value::Thermals(self.reading())),
            }],
        })
    }
}
