//! What a session is willing — and allowed — to receive.
//!
//! The manifest is the authorization: a unit reads the topics it declared,
//! and nothing else. `Subscribe` is the runtime *selection* within that set,
//! so a unit can narrow what it wakes up for without ever widening what it
//! may see. Its own keyspace needs no declaration; it owns it.

use std::collections::HashSet;

use omega_proto::Manifest;
use omega_proto::UnitName;
use omega_proto::omega::{Event, EventKind, StatePatch, StateSnapshot};
use omega_proto::{Address, Refusal};

#[derive(Debug, Clone)]
pub struct Subscriptions {
    /// The unit whose keyspace is implicitly readable and writable.
    owner: Option<UnitName>,
    /// Topics the manifest declared: the ceiling, fixed at admission.
    allowed: HashSet<String>,
    /// Topics currently wanted, or `None` for everything the ceiling allows.
    ///
    /// `None` is not an empty set. It is a session that has never selected —
    /// which is how a unit that never calls `Subscribe` behaves as its
    /// manifest reads, and the only way to say "everything" for a watcher,
    /// whose ceiling is every topic there is and cannot be listed.
    active: Option<HashSet<String>>,
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

        // The manifest was validated when it was loaded, so every value here
        // names an event this build knows.
        let allowed_events: HashSet<i32> = manifest.events.iter().copied().collect();

        Self {
            owner: Some(unit.clone()),
            active: Some(allowed.clone()),
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
            active: None,
            watches_all: true,
            allowed_events: HashSet::new(),
            active_events: HashSet::new(),
        }
    }

    /// Add to the selection. Asking for a topic the manifest did not declare
    /// is refused: subscribing is not a way to widen a grant.
    ///
    /// `replace` makes the named topics the *whole* selection rather than an
    /// addition. That is the only way a watcher can narrow — its ceiling is
    /// every topic there is, so there is no list for `Unsubscribe` to
    /// subtract from — and it is stable as the ontology grows, where naming
    /// everything unwanted would quietly let each new topic back in.
    pub fn subscribe(&mut self, topics: &[String], replace: bool) -> Result<(), Refusal> {
        for topic in topics {
            Address::parse(topic).map_err(|error| Refusal::invalid(error.to_string()))?;
            if !self.permits(topic) {
                return Err(Refusal::denied(format!(
                    "topic {topic:?} is not declared by this unit"
                )));
            }
        }

        if replace {
            self.active = Some(topics.iter().cloned().collect());
        } else if let Some(active) = self.active.as_mut() {
            active.extend(topics.iter().cloned());
        }
        // Otherwise the selection already stands for everything the ceiling
        // allows, and adding to it changes nothing.
        Ok(())
    }

    pub fn unsubscribe(&mut self, topics: &[String]) {
        // A selection standing for "everything allowed" has no list to take a
        // topic out of. `Subscribe` with `replace` is how such a session says
        // what it wants instead.
        let Some(active) = self.active.as_mut() else {
            return;
        };
        for topic in topics {
            active.remove(topic);
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
    ///
    /// The selection, not the ceiling — which is the whole difference between
    /// this and [`permits`]. `watches_all` was consulted here too, so a
    /// watcher could never narrow however it asked, and every observer of the
    /// shell socket was sent every topic the daemon held.
    ///
    /// [`permits`]: Self::permits
    pub fn wants(&self, topic: &str) -> bool {
        match &self.active {
            Some(active) => active.contains(topic) || self.owns(topic),
            None => self.permits(topic),
        }
    }

    /// A unit's own keyspace: always readable, always writable by it alone.
    pub fn owns(&self, topic: &str) -> bool {
        let (Some(owner), Ok(parsed)) = (self.owner.as_ref(), Address::parse(topic)) else {
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

    /// Read selection is filtered before leaving the authorization boundary.
    pub fn read(
        &self,
        snapshot: StateSnapshot,
        requested: &[String],
    ) -> Result<StatePatch, Refusal> {
        for topic in requested {
            Address::parse(topic).map_err(|error| Refusal::invalid(error.to_string()))?;
            if !self.permits(topic) {
                return Err(Refusal::denied(format!(
                    "topic {topic:?} is not declared by this unit"
                )));
            }
        }
        Ok(StatePatch {
            topics: snapshot
                .topics
                .into_iter()
                .filter(|topic| {
                    if requested.is_empty() {
                        self.wants(&topic.topic)
                    } else {
                        requested.contains(&topic.topic)
                    }
                })
                .collect(),
        })
    }

    pub fn select(&mut self, request: &omega_proto::omega::Subscribe) -> Result<(), Refusal> {
        let mut next = self.clone();
        next.subscribe(&request.topics, request.replace)?;
        next.subscribe_events(&request.events)?;
        *self = next;
        Ok(())
    }
}
