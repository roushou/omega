//! What time it is, where this machine is.
//!
//! Not a subsystem so much as a fact nobody else was going to broker. A unit
//! could read the clock itself, but not the zone rules — turning a timestamp
//! into "14:32" needs the tz database, and every unit carrying one to draw a
//! bar clock is the thing this daemon exists to avoid.
//!
//! Wakes once a minute, on the minute. A tick a second would wake every clock
//! on the bar sixty times an hour to redraw the same two digits, and sleeping
//! to the boundary rather than counting seconds is what keeps the displayed
//! minute from lagging the real one by up to a second.

use std::time::Duration;

use async_trait::async_trait;
use chrono::{Datelike, Local, Timelike};

use omega_proto::SystemTopic;
use omega_proto::omega::{StatePatch, StateTopic, TimeState, state_topic};

use crate::broker::{Broker, BrokerError};

#[derive(Debug, Default)]
pub struct Clock {
    primed: bool,
}

impl Clock {
    pub fn new() -> Self {
        Self::default()
    }

    /// The wall clock now, truncated to the minute.
    ///
    /// Truncated including the timestamp: a value that moved every second
    /// would be a new revision every second, and last-value-wins only helps
    /// when the value is actually the same.
    pub fn now() -> TimeState {
        Self::of(Local::now())
    }

    /// The ontology's view of one moment. Pure, so the arithmetic is testable
    /// without waiting for a clock to move.
    pub fn of(at: chrono::DateTime<Local>) -> TimeState {
        let offset = at.offset().local_minus_utc();
        TimeState {
            unix_seconds: at.timestamp() - i64::from(at.second()),
            // `%Z` is the abbreviation the platform knows this zone by. The
            // IANA name is not reliably recoverable from a `DateTime`, and an
            // abbreviation a person recognises beats a name nobody set.
            zone: at.format("%Z").to_string(),
            utc_offset_seconds: offset,
            year: at.year(),
            month: at.month(),
            day: at.day(),
            hour: at.hour(),
            minute: at.minute(),
            weekday: at.weekday().num_days_from_sunday(),
        }
    }

    /// How long until the clock reads a different minute.
    ///
    /// Never zero: waking exactly on the boundary can land a whisker early
    /// and read the minute that is ending, so this sleeps into the next one.
    fn until_next_minute(at: &chrono::DateTime<Local>) -> Duration {
        let past = u64::from(at.second());
        Duration::from_secs(60 - past.min(59))
    }

    fn patch(state: TimeState) -> StatePatch {
        StatePatch {
            topics: vec![StateTopic {
                topic: SystemTopic::Time.as_str().into(),
                revision: 0, // the Hub assigns the real revision
                value: Some(state_topic::Value::Time(state)),
            }],
        }
    }
}

#[async_trait]
impl Broker for Clock {
    fn name(&self) -> &'static str {
        "clock"
    }

    fn topics(&self) -> &'static [SystemTopic] {
        &[SystemTopic::Time]
    }

    async fn next(&mut self) -> Result<StatePatch, BrokerError> {
        // The first reading is now: a bar that started blank until the minute
        // turned would be blank for up to a minute.
        if self.primed {
            // `sleep` is cancel-safe, and losing one costs a recomputed
            // deadline rather than a missed minute — the next call sleeps to
            // whatever boundary is next from wherever the clock is then.
            tokio::time::sleep(Self::until_next_minute(&Local::now())).await;
        }
        self.primed = true;
        Ok(Self::patch(Self::now()))
    }
}
