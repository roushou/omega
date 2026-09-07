//! What a session is willing — and allowed — to receive.
//!
//! The manifest is the authorization: a unit reads the topics it declared,
//! and nothing else. `Subscribe` is the runtime *selection* within that set,
//! so a unit can narrow what it wakes up for without ever widening what it
//! may see. Its own keyspace needs no declaration; it owns it.

use std::collections::HashSet;

use omega_core::UnitName;
use omega_manifest::Manifest;
use omega_wire::omega::{Event, EventKind, StatePatch, StateSnapshot};
use omega_wire::{Refusal, Topic};

#[derive(Debug)]
pub struct Subscriptions {
    /// The unit whose keyspace is implicitly readable and writable.
    owner: Option<UnitName>,
    /// Topics the manifest declared: the ceiling, fixed at admission.
    allowed: HashSet<String>,
    /// Topics currently wanted. Starts as everything allowed, so a unit that
    /// never calls `Subscribe` behaves as its manifest reads.
    active: HashSet<String>,
    /// A watcher sees every topic. Only a debug client the operator admitted
    /// on purpose is one.
    watches_all: bool,
    /// Event kinds the manifest declared, and the ones currently wanted —
    /// the same ceiling-and-selection rule as topics.
    allowed_events: HashSet<i32>,
    active_events: HashSet<i32>,
}

impl Subscriptions {
    /// The subscription a manifest describes. The manifest was validated when
    /// it was loaded, so its topics parse.
    pub fn of(unit: &UnitName, manifest: &Manifest) -> Self {
        let allowed: HashSet<String> = manifest.state_topics.iter().cloned().collect();

        // The manifest was validated when it was loaded, so its events parse.
        let allowed_events: HashSet<i32> = manifest
            .events()
            .unwrap_or_default()
            .into_iter()
            .map(|kind| kind as i32)
            .collect();

        Self {
            owner: Some(unit.clone()),
            active: allowed.clone(),
            allowed,
            watches_all: false,
            active_events: allowed_events.clone(),
            allowed_events,
        }
    }

    /// A read-only watcher: every topic, no keyspace, no writes. This is what
    /// `--allow-debug-clients` admits a stranger as.
    pub fn watcher() -> Self {
        Self {
            owner: None,
            allowed: HashSet::new(),
            active: HashSet::new(),
            watches_all: true,
            allowed_events: HashSet::new(),
            active_events: HashSet::new(),
        }
    }

    /// Narrow (or restore) the selection. Asking for a topic the manifest did
    /// not declare is refused: subscribing is not a way to widen a grant.
    pub fn subscribe(&mut self, topics: &[String]) -> Result<(), Refusal> {
        for topic in topics {
            if !self.permits(topic) {
                return Err(Refusal::denied(format!(
                    "topic {topic:?} is not declared by this unit"
                )));
            }
        }
        self.active.extend(topics.iter().cloned());
        Ok(())
    }

    pub fn unsubscribe(&mut self, topics: &[String]) {
        for topic in topics {
            self.active.remove(topic);
        }
    }

    /// Narrow (or restore) the event selection. As with topics, the manifest
    /// is the ceiling: a unit handles the events it declared and no others.
    pub fn subscribe_events(&mut self, events: &[i32]) -> Result<(), Refusal> {
        for kind in events {
            if !self.allowed_events.contains(kind) {
                return Err(Refusal::denied(format!(
                    "event {} is not declared by this unit",
                    Self::name(*kind)
                )));
            }
        }
        self.active_events.extend(events.iter().copied());
        Ok(())
    }

    pub fn unsubscribe_events(&mut self, events: &[i32]) {
        for kind in events {
            self.active_events.remove(kind);
        }
    }

    /// Whether this session should be woken for an event.
    pub fn wants_event(&self, event: &Event) -> bool {
        self.active_events.contains(&event.kind)
    }

    fn name(kind: i32) -> String {
        EventKind::try_from(kind)
            .map(|kind| kind.as_str_name().to_string())
            .unwrap_or_else(|_| kind.to_string())
    }

    /// Whether the manifest allows this topic at all.
    pub fn permits(&self, topic: &str) -> bool {
        self.watches_all || self.allowed.contains(topic) || self.owns(topic)
    }

    /// Whether the session wants this topic right now.
    pub fn wants(&self, topic: &str) -> bool {
        self.watches_all || self.active.contains(topic) || self.owns(topic)
    }

    /// A unit's own keyspace: always readable, always writable by it alone.
    pub fn owns(&self, topic: &str) -> bool {
        let (Some(owner), Ok(parsed)) = (self.owner.as_ref(), Topic::parse(topic)) else {
            return false;
        };
        parsed.owner() == Some(owner.as_str())
    }

    /// The part of a snapshot this session should see. The mirror a unit is
    /// handed at admission is bounded by the same rule as everything after
    /// it: a unit that declared `battery` is not handed the machine's audio
    /// and network state for free.
    pub fn filter_snapshot(&self, snapshot: StateSnapshot) -> StateSnapshot {
        StateSnapshot {
            topics: snapshot
                .topics
                .into_iter()
                .filter(|topic| self.wants(&topic.topic))
                .collect(),
        }
    }

    /// The part of a patch this session should see, or `None` when none of it
    /// is wanted.
    pub fn filter(&self, patch: &StatePatch) -> Option<StatePatch> {
        let topics: Vec<_> = patch
            .topics
            .iter()
            .filter(|topic| self.wants(&topic.topic))
            .cloned()
            .collect();

        (!topics.is_empty()).then_some(StatePatch { topics })
    }

    /// The topics a `GetState` with no names should answer with.
    pub fn active(&self) -> Vec<String> {
        let mut topics: Vec<_> = self.active.iter().cloned().collect();
        topics.sort();
        topics
    }
}
