//! Assign event identity and derive state-transition events.
//! Derive transitions from published state updates so event payloads and readings agree.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use omega_proto::omega::{
    BatteryState, ClipboardEvent, CustomEvent, Event, EventKind, IdleEvent, PowerEvent, StatePatch,
    Value, event, state_topic,
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

    /// A plugin's own event. The plugin name is the one the daemon authenticated,
    /// never the one the frame carried.
    pub fn custom(&self, plugin: &str, name: &str, payload: Option<Value>) -> Event {
        self.stamp(
            EventKind::EventCustom,
            Some(event::Detail::Custom(CustomEvent {
                plugin: plugin.to_string(),
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
    mains: Option<bool>,
    idle: Option<bool>,
    clipboard: Option<String>,
}

impl Transitions {
    /// Below this the machine is low; below the second, critical. Reported
    /// once per crossing, not once per poll.
    const LOW: f64 = 0.15;
    const CRITICAL: f64 = 0.05;

    pub fn new() -> Self {
        Self::default()
    }

    /// The events a patch implies, given everything seen before it, with the
    /// detail each subscriber receives.
    pub fn of(&mut self, patch: &StatePatch) -> Vec<(EventKind, Option<event::Detail>)> {
        let mut events = Vec::new();
        for topic in &patch.topics {
            match topic.value.as_ref() {
                Some(state_topic::Value::Battery(battery)) => events.extend(self.battery(*battery)),
                Some(state_topic::Value::Mains(mains)) => {
                    if let Some(previous) = self.mains.replace(mains.connected)
                        && previous != mains.connected
                    {
                        events.push((
                            if mains.connected {
                                EventKind::EventAcPlugged
                            } else {
                                EventKind::EventAcUnplugged
                            },
                            Some(event::Detail::Power(PowerEvent::default())),
                        ));
                    }
                }
                Some(state_topic::Value::Idle(idle)) => {
                    if let Some(previous) = self.idle.replace(idle.idle)
                        && previous != idle.idle
                    {
                        events.push((
                            if idle.idle {
                                EventKind::EventIdleEntered
                            } else {
                                EventKind::EventIdleExited
                            },
                            Some(event::Detail::Idle(IdleEvent {})),
                        ));
                    }
                }
                Some(state_topic::Value::Clipboard(clipboard)) => {
                    if let Some(previous) = self.clipboard.replace(clipboard.text.clone())
                        && previous != clipboard.text
                    {
                        events.push((
                            EventKind::EventClipboardChanged,
                            Some(event::Detail::Clipboard(ClipboardEvent {})),
                        ));
                    }
                }
                None if topic.topic == "battery" => self.battery = None,
                None if topic.topic == "mains" => self.mains = None,
                None if topic.topic == "idle" => self.idle = None,
                None if topic.topic == "clipboard" => self.clipboard = None,
                _ => {}
            }
        }
        events
    }

    fn battery(&mut self, next: BatteryState) -> Vec<(EventKind, Option<event::Detail>)> {
        let previous = self.battery.replace(next);

        let Some(previous) = previous else {
            // The first reading is the state of the world, not a change in it.
            return Vec::new();
        };

        let mut events = Vec::new();

        if Self::crossed(previous.level, next.level, Self::CRITICAL) {
            events.push((EventKind::EventBatteryCritical, Self::power(next.level)));
        } else if Self::crossed(previous.level, next.level, Self::LOW) {
            events.push((EventKind::EventBatteryLow, Self::power(next.level)));
        }

        events
    }

    fn power(battery_percent: f64) -> Option<event::Detail> {
        Some(event::Detail::Power(PowerEvent { battery_percent }))
    }

    /// Whether a level fell past a threshold it was above.
    fn crossed(previous: f64, next: f64, threshold: f64) -> bool {
        previous > threshold && next <= threshold
    }
}
