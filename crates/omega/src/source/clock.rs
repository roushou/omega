//! What time it is.

use std::fmt;

use omega_proto::omega::TimeState;

use crate::context::Context;
use crate::source::reads;

/// The wall clock, in the machine's own zone.
///
/// The daemon applies the zone rules, so a unit draws the time without
/// carrying a calendar:
///
/// ```no_run
/// # use omega::{Clock, Text, Ui, Widget};
/// #[derive(omega::Widget)]
/// struct Bar {
///     clock: Clock,
/// }
///
/// impl Widget for Bar {
///     fn render(&self) -> Ui {
///         Text::new(self.clock.time()).into()
///     }
/// }
/// ```
///
/// **Minutes, not seconds.** The topic has minute resolution, so a widget
/// holding this is asked to draw once a minute rather than sixty times to
/// redraw the same two digits. A seconds display needs a granularity a unit
/// can ask for, which the manifest cannot yet carry.
#[derive(Debug)]
pub struct Clock {
    context: Context,
}

reads!(Clock, TimeState);

impl Clock {
    /// The hour, 0..23.
    pub fn hour(&self) -> u32 {
        self.read().map(|time| time.hour).unwrap_or(0)
    }

    /// The minute, 0..59.
    pub fn minute(&self) -> u32 {
        self.read().map(|time| time.minute).unwrap_or(0)
    }

    pub fn year(&self) -> i32 {
        self.read().map(|time| time.year).unwrap_or(0)
    }

    /// The month, 1..12.
    pub fn month(&self) -> u32 {
        self.read().map(|time| time.month).unwrap_or(1)
    }

    /// The day of the month, 1..31.
    pub fn day(&self) -> u32 {
        self.read().map(|time| time.day).unwrap_or(1)
    }

    pub fn weekday(&self) -> Weekday {
        Weekday::of(self.read().map(|time| time.weekday).unwrap_or(0))
    }

    /// What zone the daemon read it in — the abbreviation the platform knows,
    /// like `CEST`.
    pub fn zone(&self) -> String {
        self.read().map(|time| time.zone).unwrap_or_default()
    }

    /// Prints itself as `14:32`.
    ///
    /// The two ready-made forms are here because a format string would be a
    /// parser, and a bar wants one of these:
    ///
    /// ```
    /// # use omega::Clock;
    /// # fn show(clock: &Clock) -> String {
    /// format!("{} {}", clock.weekday().short(), clock.time())
    /// # }
    /// ```
    pub fn time(&self) -> impl fmt::Display + use<> {
        HourMinute {
            hour: self.hour(),
            minute: self.minute(),
        }
    }

    /// Prints itself as `2026-09-08`.
    pub fn date(&self) -> impl fmt::Display + use<> {
        YearMonthDay {
            year: self.year(),
            month: self.month(),
            day: self.day(),
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct HourMinute {
    hour: u32,
    minute: u32,
}

impl fmt::Display for HourMinute {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:02}:{:02}", self.hour, self.minute)
    }
}

#[derive(Debug, Clone, Copy)]
struct YearMonthDay {
    year: i32,
    month: u32,
    day: u32,
}

impl fmt::Display for YearMonthDay {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }
}

/// A day of the week, so a bar can name one without a lookup table of its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Weekday {
    Sunday,
    Monday,
    Tuesday,
    Wednesday,
    Thursday,
    Friday,
    Saturday,
}

impl Weekday {
    /// From the wire's numbering, where zero is Sunday. A number outside the
    /// week is Sunday rather than a panic: a widget should draw a wrong day,
    /// not take the unit down.
    fn of(day: u32) -> Self {
        match day % 7 {
            1 => Self::Monday,
            2 => Self::Tuesday,
            3 => Self::Wednesday,
            4 => Self::Thursday,
            5 => Self::Friday,
            6 => Self::Saturday,
            _ => Self::Sunday,
        }
    }

    /// `Mon`.
    pub fn short(self) -> &'static str {
        &self.long()[..3]
    }

    /// `Monday`.
    pub fn long(self) -> &'static str {
        match self {
            Self::Sunday => "Sunday",
            Self::Monday => "Monday",
            Self::Tuesday => "Tuesday",
            Self::Wednesday => "Wednesday",
            Self::Thursday => "Thursday",
            Self::Friday => "Friday",
            Self::Saturday => "Saturday",
        }
    }
}

impl fmt::Display for Weekday {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.long())
    }
}
