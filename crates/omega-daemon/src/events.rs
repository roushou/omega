//! Events: what actually happened.
//!
//! The daemon owns event identity the way it owns state revisions — a
//! producer says what happened, the daemon says when and in what order.
//!
//! Most events are *transitions of state*, not a second thing a source has to
//! remember to announce. Deriving them from the state plane means "the AC was
//! unplugged" cannot disagree with `battery.charging`: they are the same
//! fact, reported twice.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use omega_proto::omega::{
    BatteryState, CustomEvent, Event, EventKind, PowerEvent, StatePatch, Value, event, state_topic,
};

/// Assigns every event its id and timestamp.
#[derive(Debug, Default)]
pub struct EventStamp {
    next: AtomicU64,
}

impl EventStamp {
    pub fn new() -> Self {
        Self::default()
    }

    /// Stamp an event with the next id and the current time.
    pub fn stamp(&self, kind: EventKind, detail: Option<event::Detail>) -> Event {
        Event {
            id: self.next.fetch_add(1, Ordering::Relaxed) + 1,
            timestamp_ns: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|since| since.as_nanos() as u64)
                .unwrap_or_default(),
            kind: kind as i32,
            detail,
        }
    }

    /// A unit's own event. The unit name is the one the daemon authenticated,
    /// never the one the frame carried.
    pub fn custom(&self, unit: &str, name: &str, payload: Option<Value>) -> Event {
        self.stamp(
            EventKind::EventCustom,
            Some(event::Detail::Custom(CustomEvent {
                unit: unit.to_string(),
                name: name.to_string(),
                payload,
            })),
        )
    }
}

/// Turns state changes into the events the ontology names.
#[derive(Debug, Default)]
pub struct Transitions {
    battery: Option<BatteryState>,
}

impl Transitions {
    /// Below this the machine is low; below the second, critical. Reported
    /// once per crossing, not once per poll.
    const LOW: f64 = 0.15;
    const CRITICAL: f64 = 0.05;

    pub fn new() -> Self {
        Self::default()
    }

    /// The events a patch implies, given everything seen before it.
    pub fn of(&mut self, patch: &StatePatch) -> Vec<EventKind> {
        patch
            .topics
            .iter()
            .filter_map(|topic| match topic.value.as_ref()? {
                state_topic::Value::Battery(battery) => Some(self.battery(*battery)),
                _ => None,
            })
            .flatten()
            .collect()
    }

    fn battery(&mut self, next: BatteryState) -> Vec<EventKind> {
        let previous = self.battery.replace(next);

        let Some(previous) = previous else {
            // The first reading is the state of the world, not a change in it.
            return Vec::new();
        };

        let mut events = Vec::new();

        if next.charging != previous.charging {
            events.push(if next.charging {
                EventKind::EventAcPlugged
            } else {
                EventKind::EventAcUnplugged
            });
        }

        if Self::crossed(previous.level, next.level, Self::CRITICAL) {
            events.push(EventKind::EventBatteryCritical);
        } else if Self::crossed(previous.level, next.level, Self::LOW) {
            events.push(EventKind::EventBatteryLow);
        }

        events
    }

    /// Whether a level fell past a threshold it was above.
    fn crossed(previous: f64, next: f64, threshold: f64) -> bool {
        previous > threshold && next <= threshold
    }
}

/// Power events carry no detail beyond their kind; this is the empty body the
/// schema specifies for them.
#[derive(Debug)]
pub struct PowerDetail;

impl PowerDetail {
    pub fn of(kind: EventKind) -> Option<event::Detail> {
        matches!(
            kind,
            EventKind::EventAcPlugged
                | EventKind::EventAcUnplugged
                | EventKind::EventBatteryLow
                | EventKind::EventBatteryCritical
                | EventKind::EventLidClosed
                | EventKind::EventLidOpened
                | EventKind::EventSuspend
                | EventKind::EventResume
        )
        .then_some(event::Detail::Power(PowerEvent {}))
    }
}
