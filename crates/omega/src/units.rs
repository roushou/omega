//! The units a reading is in.
//!
//! A battery level is not an `f64`. The wire carries `0.0 .. 1.0` and a
//! widget wants "80%", and every plugin that multiplies by a hundred itself
//! is a plugin that can get it wrong. These types carry the scale so nobody
//! has to remember it, and print themselves so nobody has to format it.

use std::fmt;
use std::time::Duration;

/// A fraction of something, printed as a whole percent.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct Percent(f64);

impl Percent {
    pub const ZERO: Self = Self(0.0);
    pub const FULL: Self = Self(1.0);

    /// From the wire's `0.0 .. 1.0`, clamped: a source that reports 1.3 is
    /// wrong, and a widget drawing a bar 130% wide is wrong too.
    pub fn of(fraction: f64) -> Self {
        Self(fraction.clamp(0.0, 1.0))
    }

    /// From a whole percent, as a person says it: `Percent::whole(80)`.
    pub fn whole(percent: u8) -> Self {
        Self::of(f64::from(percent) / 100.0)
    }

    /// `0.0 .. 1.0`, for drawing.
    pub fn fraction(self) -> f64 {
        self.0
    }

    /// `0 ..= 100`, for comparing against a threshold someone typed.
    pub fn whole_percent(self) -> u8 {
        (self.0 * 100.0).round() as u8
    }
}

impl fmt::Display for Percent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:.0}%", self.0 * 100.0)
    }
}

impl PartialEq<u8> for Percent {
    fn eq(&self, whole: &u8) -> bool {
        self.whole_percent() == *whole
    }
}

impl PartialOrd<u8> for Percent {
    fn partial_cmp(&self, whole: &u8) -> Option<std::cmp::Ordering> {
        self.whole_percent().partial_cmp(whole)
    }
}

/// A span of time, printed the way a bar shows it: `2h 40m`, `12m`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Remaining(Duration);

impl Remaining {
    pub fn of(duration: Duration) -> Self {
        Self(duration)
    }

    /// `None` for zero, which is how the wire spells "unknown".
    pub fn seconds(seconds: u32) -> Option<Self> {
        match seconds {
            0 => None,
            _ => Some(Self(Duration::from_secs(u64::from(seconds)))),
        }
    }

    pub fn duration(self) -> Duration {
        self.0
    }
}

impl fmt::Display for Remaining {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let minutes = self.0.as_secs() / 60;
        match (minutes / 60, minutes % 60) {
            (0, minutes) => write!(f, "{minutes}m"),
            (hours, 0) => write!(f, "{hours}h"),
            (hours, minutes) => write!(f, "{hours}h {minutes}m"),
        }
    }
}

/// A quantity of bytes, printed the way a panel shows it: `7.5 GiB`, `912 MiB`.
///
/// Binary units, because that is what `/proc/meminfo` and `statvfs` report
/// and what every other tool on the machine prints.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct Bytes(u64);

impl Bytes {
    pub const ZERO: Self = Self(0);

    const UNITS: [(&'static str, u64); 5] = [
        ("TiB", 1 << 40),
        ("GiB", 1 << 30),
        ("MiB", 1 << 20),
        ("KiB", 1 << 10),
        ("B", 1),
    ];

    pub const fn of(bytes: u64) -> Self {
        Self(bytes)
    }

    pub const fn count(self) -> u64 {
        self.0
    }

    /// What fraction of a whole this is. `None` where the whole is nought,
    /// which is a filesystem that reported nothing rather than a full one.
    pub fn share_of(self, whole: Bytes) -> Option<Percent> {
        match whole.0 {
            0 => None,
            total => Some(Percent::of(self.0 as f64 / total as f64)),
        }
    }

    /// This much less that much, floored at nought — how "used" is computed
    /// from a total and what is free, without a widget underflowing when a
    /// source reports them a moment apart.
    pub fn less(self, other: Bytes) -> Self {
        Self(self.0.saturating_sub(other.0))
    }
}

impl fmt::Display for Bytes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (unit, size) = Self::UNITS
            .iter()
            .copied()
            .find(|(_, size)| self.0 >= *size)
            // Nought is bytes, not a panic.
            .unwrap_or(("B", 1));

        match unit {
            // A count of bytes has no fractional part worth showing.
            "B" => write!(f, "{} B", self.0),
            _ => write!(f, "{:.1} {unit}", self.0 as f64 / size as f64),
        }
    }
}

/// How long the machine has been up, printed the way an uptime is said:
/// `3d 4h`, `4h 12m`, `12m`.
///
/// Distinct from [`Remaining`] because it counts the other way and reads at a
/// different scale: nobody says a machine has been up "2h 40m" once it has
/// been up for days.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Uptime(Duration);

impl Uptime {
    pub fn of(duration: Duration) -> Self {
        Self(duration)
    }

    pub fn seconds(seconds: u64) -> Self {
        Self(Duration::from_secs(seconds))
    }

    pub fn duration(self) -> Duration {
        self.0
    }
}

impl fmt::Display for Uptime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let minutes = self.0.as_secs() / 60;
        let (hours, minutes) = (minutes / 60, minutes % 60);
        let (days, hours) = (hours / 24, hours % 24);

        match (days, hours, minutes) {
            (0, 0, minutes) => write!(f, "{minutes}m"),
            (0, hours, minutes) => write!(f, "{hours}h {minutes}m"),
            (days, hours, _) => write!(f, "{days}d {hours}h"),
        }
    }
}

/// Bytes per second, printed the way a monitor shows it: `1.2 MiB/s`.
///
/// A newtype over [`Bytes`] rather than a bare count, so a widget cannot draw
/// a rate as a total or vice versa — they read the same and mean different
/// things.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct Rate(Bytes);

impl Rate {
    pub const ZERO: Self = Self(Bytes::ZERO);

    pub const fn of(bytes_per_second: u64) -> Self {
        Self(Bytes::of(bytes_per_second))
    }

    /// How much, without the "per second".
    pub const fn amount(self) -> Bytes {
        self.0
    }

    pub const fn count(self) -> u64 {
        self.0.count()
    }
}

impl fmt::Display for Rate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/s", self.0)
    }
}

/// How hot something is, printed the way a bar shows it: `44°C`.
///
/// hwmon reports thousandths of a degree, which is three digits of precision
/// nothing draws and a new revision on every flutter. This carries what the
/// kernel said and rounds when it prints.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct Temperature(i32);

impl Temperature {
    pub const fn of_millicelsius(millicelsius: i32) -> Self {
        Self(millicelsius)
    }

    pub fn celsius(self) -> f64 {
        f64::from(self.0) / 1000.0
    }

    /// Whole degrees, for comparing against a threshold somebody typed.
    pub fn whole_celsius(self) -> i32 {
        (self.celsius()).round() as i32
    }

    pub const fn millicelsius(self) -> i32 {
        self.0
    }
}

impl fmt::Display for Temperature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}°C", self.whole_celsius())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_print_in_the_largest_unit_that_fits() {
        assert_eq!(Bytes::of(0).to_string(), "0 B");
        assert_eq!(Bytes::of(512).to_string(), "512 B");
        assert_eq!(Bytes::of(1 << 10).to_string(), "1.0 KiB");
        assert_eq!(Bytes::of(3 << 20).to_string(), "3.0 MiB");
        assert_eq!(Bytes::of(12_282_912_768).to_string(), "11.4 GiB");
    }

    #[test]
    fn used_is_a_subtraction_that_cannot_underflow() {
        // Total and available come from two reads of the same file, and a
        // source that reports more free than it has must not wrap to 16 EiB.
        let total = Bytes::of(1 << 30);
        assert_eq!(total.less(Bytes::of(1 << 29)), Bytes::of(1 << 29));
        assert_eq!(total.less(Bytes::of(1 << 31)), Bytes::ZERO);
    }

    #[test]
    fn a_share_of_nothing_is_unknown_rather_than_full() {
        assert_eq!(Bytes::of(0).share_of(Bytes::ZERO), None);
        assert_eq!(
            Bytes::of(1 << 29).share_of(Bytes::of(1 << 30)),
            Some(Percent::of(0.5))
        );
    }

    #[test]
    fn a_rate_is_an_amount_with_a_second_attached() {
        assert_eq!(Rate::of(0).to_string(), "0 B/s");
        assert_eq!(Rate::of(1536).to_string(), "1.5 KiB/s");
        assert_eq!(Rate::of(3 << 20).amount(), Bytes::of(3 << 20));
    }

    #[test]
    fn a_temperature_prints_in_whole_degrees() {
        assert_eq!(Temperature::of_millicelsius(44_000).to_string(), "44°C");
        // Rounded, not truncated: 44.6 is nearer 45.
        assert_eq!(Temperature::of_millicelsius(44_600).to_string(), "45°C");
        // Signed, because a sensor outdoors can be below freezing.
        assert_eq!(Temperature::of_millicelsius(-5_500).to_string(), "-6°C");
    }

    #[test]
    fn uptime_reads_at_the_scale_it_has_reached() {
        assert_eq!(Uptime::seconds(600).to_string(), "10m");
        assert_eq!(Uptime::seconds(3 * 3600 + 600).to_string(), "3h 10m");
        assert_eq!(Uptime::seconds(50 * 3600).to_string(), "2d 2h");
    }
}
