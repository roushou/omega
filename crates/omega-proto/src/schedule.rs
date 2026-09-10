//! How often the daemon does something on its own.
//!
//! One grammar, read by both ends: the config plane writes a cadence into a
//! document and the daemon reads it back out, through this parser.
//!
//! The grammar is closed and small: `every <n><s|m|h|d>`. Cron is not in it,
//! and [`Cadence::parse`] refuses `"0 9 * * *"` by name rather than accepting
//! a schedule that would never fire.

use std::fmt;
use std::time::Duration;

use crate::omega::{Action, Event, Schedule, event};

/// How often a schedule fires.
///
/// A duration, not a wall-clock time: "every ten minutes" is answerable
/// without a timezone, and "at nine" is not. A schedule that must land on a
/// particular hour builds on the `time` topic instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Cadence(Duration);

impl Cadence {
    /// The word every cadence starts with.
    const EVERY: &'static str = "every";

    /// The shortest period a schedule may declare.
    ///
    /// A floor rather than a guess: below a second the interval is shorter
    /// than the work it triggers, and a mistyped `every 0s` would spin the
    /// daemon against a unit until somebody noticed.
    pub const FLOOR: Duration = Duration::from_secs(1);

    /// The units a period may be written in, longest first — which is also
    /// the order [`Display`] tries them in, so a period is written in the
    /// largest unit that divides it exactly.
    ///
    /// [`Display`]: fmt::Display
    const UNITS: &'static [(char, u64)] = &[('d', 86_400), ('h', 3_600), ('m', 60), ('s', 1)];

    /// The cadence of a period, rounded to whole seconds.
    pub fn of(period: Duration) -> Result<Self, CadenceError> {
        match period < Self::FLOOR {
            true => Err(CadenceError::TooFast(period)),
            false => Ok(Self(Duration::from_secs(period.as_secs()))),
        }
    }

    pub const fn period(self) -> Duration {
        self.0
    }

    /// Named periods, for a config to declare a cadence without writing one
    /// out as a string and without handling an error it cannot hit.
    ///
    /// Each panics on a count of nought. That is a mistake in a config rather
    /// than a cadence, and the config plane is a program that runs at build
    /// time with no side effects — so it fails the build, with a message,
    /// and never reaches a machine. The parser still refuses `every 0s`,
    /// because a document can also arrive from somewhere that was not
    /// compiled.
    pub fn seconds(count: u32) -> Self {
        Self::counted(count, 1)
    }

    pub fn minutes(count: u32) -> Self {
        Self::counted(count, 60)
    }

    pub fn hours(count: u32) -> Self {
        Self::counted(count, 3_600)
    }

    pub fn days(count: u32) -> Self {
        Self::counted(count, 86_400)
    }

    fn counted(count: u32, seconds: u64) -> Self {
        assert!(count > 0, "a cadence of nought never fires");
        Self(Duration::from_secs(u64::from(count) * seconds))
    }

    /// Read a cadence as a document writes it.
    pub fn parse(cadence: &str) -> Result<Self, CadenceError> {
        let cadence = cadence.trim();

        let Some(period) = cadence.strip_prefix(Self::EVERY) else {
            return Err(CadenceError::NotACadence(cadence.to_string()));
        };

        let period = period.trim();
        let Some(unit) = period.chars().last() else {
            return Err(CadenceError::NoPeriod(cadence.to_string()));
        };

        let Some((_, seconds)) = Self::UNITS.iter().find(|(name, _)| *name == unit) else {
            return Err(CadenceError::UnknownUnit {
                cadence: cadence.to_string(),
                unit,
            });
        };

        let count: u64 = period[..period.len() - unit.len_utf8()]
            .trim()
            .parse()
            .map_err(|_| CadenceError::NoPeriod(cadence.to_string()))?;

        // A count large enough to overflow is not a cadence anybody meant,
        // and saturating it into "every 584 billion years" would hide the
        // typo behind a schedule that never fires.
        let period = count
            .checked_mul(*seconds)
            .map(Duration::from_secs)
            .ok_or_else(|| CadenceError::NoPeriod(cadence.to_string()))?;

        Self::of(period)
    }
}

impl fmt::Display for Cadence {
    /// In the largest unit that divides the period exactly, so a document
    /// says `every 10m` rather than `every 600s`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let seconds = self.0.as_secs();
        let (unit, size) = Self::UNITS
            .iter()
            .find(|(_, size)| seconds % size == 0)
            .copied()
            // `s` divides everything, so the search cannot come up empty.
            .unwrap_or(('s', 1));

        write!(f, "{} {}{unit}", Self::EVERY, seconds / size)
    }
}

/// A cadence a document declared and the daemon cannot fire.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CadenceError {
    #[error("cadence {0:?} must be shaped `every <n><s|m|h|d>` (cron is not read)")]
    NotACadence(String),
    #[error("cadence {0:?} names no period")]
    NoPeriod(String),
    #[error("cadence {cadence:?} is counted in {unit:?}, which is not one of s, m, h, d")]
    UnknownUnit { cadence: String, unit: char },
    #[error("a schedule may not fire more often than once a second, and {0:?} would")]
    TooFast(Duration),
}

impl Schedule {
    /// A schedule that fires an action on a cadence.
    pub fn new(id: impl Into<String>, cadence: Cadence, action: Action) -> Self {
        Self {
            id: id.into(),
            cadence: cadence.to_string(),
            action: Some(action),
        }
    }

    /// A schedule that only announces itself.
    ///
    /// What a unit reacts to when the thing to be done is more than one
    /// action names — the document still owns how often, which is the part
    /// that belongs to whoever runs the machine rather than to the plugin.
    pub fn announcing(id: impl Into<String>, cadence: Cadence) -> Self {
        Self {
            id: id.into(),
            cadence: cadence.to_string(),
            action: None,
        }
    }

    /// How often this schedule fires, or why it cannot.
    pub fn parsed(&self) -> Result<Cadence, CadenceError> {
        Cadence::parse(&self.cadence)
    }
}

impl Event {
    /// The schedule that fired, if this event is one firing.
    ///
    /// An accessor rather than a match on the oneof at every call site: a
    /// reaction registered for `EVENT_SCHEDULE_FIRED` hears *every* schedule,
    /// so telling them apart by id is the first thing it does, and it should
    /// not have to reach into the wire types to do it.
    pub fn schedule(&self) -> Option<&str> {
        match self.detail.as_ref()? {
            event::Detail::Schedule(fired) => Some(fired.schedule_id.as_str()),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cadence_round_trips_through_its_written_form() {
        for written in ["every 1s", "every 30s", "every 10m", "every 2h", "every 1d"] {
            let cadence = Cadence::parse(written).expect(written);
            assert_eq!(cadence.to_string(), written);
        }
    }

    #[test]
    fn a_period_is_written_in_the_largest_unit_that_divides_it() {
        // What the config plane writes when it is handed a Duration: 600
        // seconds is ten minutes, and a document saying so is one a person
        // can check against what they meant.
        let ten_minutes = Cadence::of(Duration::from_secs(600)).unwrap();
        assert_eq!(ten_minutes.to_string(), "every 10m");

        // And a period no larger unit divides stays in seconds.
        let ninety = Cadence::of(Duration::from_secs(90)).unwrap();
        assert_eq!(ninety.to_string(), "every 90s");
    }

    #[test]
    fn cron_is_refused_by_name() {
        // The failure this whole grammar exists to prevent: a schedule that
        // parses as something and fires as nothing.
        let err = Cadence::parse("0 9 * * *").unwrap_err();
        assert!(err.to_string().contains("cron is not read"), "{err}");
    }

    #[test]
    fn a_cadence_with_no_period_is_refused() {
        assert!(matches!(
            Cadence::parse("every"),
            Err(CadenceError::NoPeriod(_))
        ));
        assert!(matches!(
            Cadence::parse("every m"),
            Err(CadenceError::NoPeriod(_))
        ));
        assert!(matches!(
            Cadence::parse("every 10"),
            Err(CadenceError::UnknownUnit { unit: '0', .. })
        ));
    }

    #[test]
    fn a_unit_this_grammar_does_not_know_is_refused() {
        // Weeks and years are not in the table. Reading `every 2w` as two
        // seconds would be the worst of both.
        assert!(matches!(
            Cadence::parse("every 2w"),
            Err(CadenceError::UnknownUnit { unit: 'w', .. })
        ));
    }

    #[test]
    fn nothing_may_fire_faster_than_the_floor() {
        assert!(matches!(
            Cadence::parse("every 0s"),
            Err(CadenceError::TooFast(_))
        ));
        assert!(matches!(
            Cadence::of(Duration::from_millis(200)),
            Err(CadenceError::TooFast(_))
        ));
    }

    #[test]
    fn a_count_too_large_to_hold_is_a_typo_not_a_schedule() {
        assert!(Cadence::parse(&format!("every {}d", u64::MAX)).is_err());
    }

    #[test]
    fn whitespace_is_not_part_of_the_grammar() {
        assert_eq!(
            Cadence::parse("  every   10m  ").unwrap(),
            Cadence::of(Duration::from_secs(600)).unwrap()
        );
    }

    #[test]
    fn a_named_period_is_the_period_it_names() {
        assert_eq!(Cadence::minutes(10), Cadence::parse("every 10m").unwrap());
        assert_eq!(Cadence::hours(1), Cadence::parse("every 1h").unwrap());
        assert_eq!(Cadence::days(1), Cadence::parse("every 1d").unwrap());
        assert_eq!(Cadence::seconds(90).to_string(), "every 90s");
    }

    #[test]
    #[should_panic(expected = "never fires")]
    fn a_cadence_of_nought_fails_the_config_that_declares_it() {
        Cadence::minutes(0);
    }

    #[test]
    fn a_schedule_carries_the_cadence_it_was_built_with() {
        let every_minute = Cadence::of(Duration::from_secs(60)).unwrap();
        let schedule = Schedule::announcing("refresh", every_minute);

        assert_eq!(schedule.cadence, "every 1m");
        assert_eq!(schedule.parsed().unwrap(), every_minute);
        assert!(schedule.action.is_none());
    }

    #[test]
    fn only_a_firing_names_a_schedule() {
        use crate::omega::{EventKind, ScheduleFired};

        let fired = Event {
            id: 1,
            timestamp_ns: 0,
            kind: EventKind::EventScheduleFired as i32,
            detail: Some(event::Detail::Schedule(ScheduleFired {
                schedule_id: "refresh".into(),
            })),
        };
        assert_eq!(fired.schedule(), Some("refresh"));

        let plugged = Event {
            id: 2,
            timestamp_ns: 0,
            kind: EventKind::EventAcPlugged as i32,
            detail: None,
        };
        assert_eq!(plugged.schedule(), None);
    }
}
