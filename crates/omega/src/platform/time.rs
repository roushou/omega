//! Local date and time readings with minute resolution.

crate::wiring::reading! {
    /// Local date and time with minute resolution.
    Clock: omega_proto::omega::TimeState
}

use std::fmt;

/// Local date and time, updated once per minute.
/// Second-resolution updates are not supported.
///
/// ```no_run
/// # use omega::platform::time::Clock;
/// # use omega::ui::Text;
/// # use omega::{View, Surface};
/// #[derive(omega::Surface)]
/// struct ClockView { clock: Clock }
/// impl Surface for ClockView {
///     type Model = ();
///     type Message = std::convert::Infallible;
///     type Effects = ();
///     fn update(&self, _: &mut (), message: Self::Message, _: &()) -> omega::surface::Task<Self::Message> {
///         match message {}
///     }
///
///     fn render(&self, _: &(), _: &omega::surface::Events<Self::Message>) -> View { Text::new(self.clock.time()).into() }
/// }
/// ```
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

    /// Time-zone abbreviation reported by the platform, such as `CEST`.
    pub fn zone(&self) -> String {
        self.read().map(|time| time.zone).unwrap_or_default()
    }

    /// Return a display value formatted as `HH:MM` in 24-hour time.
    ///
    /// ```
    /// # use omega::platform::time::Clock;
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

    /// Return a display value formatted as `YYYY-MM-DD`.
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

/// Day of the week with English display labels.
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
    /// Decode Sunday-based weekday numbers; unknown values default to Sunday.
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

    /// Three-letter English label, such as `Mon`.
    pub fn short(self) -> &'static str {
        &self.long()[..3]
    }

    /// Full English label, such as `Monday`.
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
