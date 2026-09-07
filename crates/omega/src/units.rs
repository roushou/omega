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
