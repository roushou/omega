//! Publish local date and time at minute boundaries using platform time-zone rules.

use std::time::Duration;

use async_trait::async_trait;
use chrono::{Datelike, Local, Timelike};

use omega_proto::SystemTopic;
use omega_proto::omega::{StatePatch, StateTopic, TimeState, state_topic};

use crate::broker::{Broker, BrokerError};

#[derive(Debug, Default)]
pub struct Clock {}

impl Clock {
    pub fn new() -> Self {
        Self::default()
    }

    /// Read wall-clock time with minute precision, including its timestamp.
    pub fn now() -> TimeState {
        Self::of(Local::now())
    }

    /// The ontology's view of one moment. Pure, so the arithmetic is testable
    /// without waiting for a clock to move.
    pub fn of(at: chrono::DateTime<Local>) -> TimeState {
        let offset = at.offset().local_minus_utc();
        TimeState {
            unix_seconds: at.timestamp() - i64::from(at.second()),
            // Use the platform time-zone abbreviation; the IANA name is not available here.
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

    /// Delay until just after the next minute boundary to avoid rereading the current minute.
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

    async fn wake(&mut self) -> Result<(), BrokerError> {
        // Recompute the wall-clock boundary after cancellation.
        tokio::time::sleep(Self::until_next_minute(&Local::now())).await;
        Ok(())
    }

    async fn read(&mut self) -> Result<StatePatch, BrokerError> {
        Ok(Self::patch(Self::now()))
    }
}
