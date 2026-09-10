//! How hot the machine is, from `hwmon`.
//!
//! Every chip that reports a temperature or a fan appears under
//! `/sys/class/hwmon`, and what is there varies wildly: this laptop offers the
//! CPU die, the GPU edge, the SSD, the wireless card, and six ACPI zones that
//! all read the same number. Nothing here tries to pick the important one —
//! that is a decision about what to draw, and it belongs to whatever draws it.
//!
//! Two asymmetries the kernel forces and this has to respect. A sensor that is
//! present but disabled fails its read with `ENXIO` rather than reporting
//! anything, so a read error is a sensor to skip and not a broker that is
//! broken. And a temperature of nought is a sensor that is not really there,
//! while a fan of nought is a fan that is stopped — which is a real state, and
//! the interesting one on a quiet machine.

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
    /// How often the machine is felt.
    ///
    /// Slower than the CPU reading: a die warms and cools over seconds, and
    /// this is a directory walk and a dozen file reads rather than one.
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

    /// One reading. A chip that cannot be read is skipped rather than failing
    /// the lot: hwmon is a collection of independent devices and one of them
    /// going away is not the others going away.
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
                    // Nought is a sensor that is not there. Reporting it would
                    // put a frozen CPU beside a warm one in every list.
                    if millicelsius != 0 {
                        sensors.push(Sensor {
                            chip: name.clone(),
                            label: Self::label(&chip, "temp", index),
                            millicelsius,
                        });
                    }
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

    /// How many of each kind to look for per chip. hwmon numbers them from one
    /// with no upper bound stated anywhere; this laptop's busiest chip has
    /// eight, and a chip with more than this is reporting more than anything
    /// would draw.
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
