//! Apply runtime subscription selection within the manifest's authorization ceiling.
//! A plugin may always access its own keyspace.

use std::collections::HashSet;

use omega_proto::Manifest;
use omega_proto::PluginName;
use omega_proto::omega::{Event, EventKind, StatePatch, StateSnapshot};
use omega_proto::{Address, Refusal};

#[derive(Debug, Clone)]
pub struct Subscriptions {
    /// The plugin whose keyspace is implicitly readable and writable.
    owner: Option<PluginName>,
    /// Topics the manifest declared: the ceiling, fixed at admission.
    allowed: HashSet<String>,
    /// Selected topics, or `None` for the complete authorized set.
    active: Option<HashSet<String>>,
    /// Create an all-topics subscription for an authorized observer.
    watches_all: bool,
    /// Event kinds the manifest declared, and the ones currently wanted —
    /// the same ceiling-and-selection rule as topics.
    allowed_events: HashSet<i32>,
    active_events: HashSet<i32>,
}

impl Subscriptions {
    /// The subscription a manifest describes. The manifest was validated when
    /// it was loaded, so its topics parse.
    pub fn of(plugin: &PluginName, manifest: &Manifest) -> Self {
        let allowed: HashSet<String> = manifest.state_topics.iter().cloned().collect();

        // The manifest was validated when it was loaded, so every value here
        // names an event this build knows.
        let allowed_events: HashSet<i32> = manifest.events.iter().copied().collect();

        Self {
            owner: Some(plugin.clone()),
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

    /// Add authorized topics to the selection, or replace the selection when requested.
    /// Topics outside the manifest ceiling are refused. Replacement also allows
    /// an all-topics observer to narrow its subscription.
    pub fn subscribe(&mut self, topics: &[String], replace: bool) -> Result<(), Refusal> {
        for topic in topics {
            topic
                .parse::<Address>()
                .map_err(|error| Refusal::invalid(error.to_string()))?;
            if !self.permits(topic) {
                return Err(Refusal::denied(format!(
                    "topic {topic:?} is not declared by this plugin"
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
        // Narrow an unrestricted subscription with Subscribe replace, not individual removals.
        let Some(active) = self.active.as_mut() else {
            return;
        };
        for topic in topics {
            active.remove(topic);
        }
    }

    /// Narrow (or restore) the event selection. As with topics, the manifest
    /// is the ceiling: a plugin handles the events it declared and no others.
    pub fn subscribe_events(&mut self, events: &[i32]) -> Result<(), Refusal> {
        for kind in events {
            if !self.allowed_events.contains(kind) {
                return Err(Refusal::denied(format!(
                    "event {} is not declared by this plugin",
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

    /// Whether the current selection includes this topic.
    /// [`permits`](Self::permits) separately checks the authorization ceiling.
    pub fn wants(&self, topic: &str) -> bool {
        match &self.active {
            Some(active) => active.contains(topic) || self.owns(topic),
            None => self.permits(topic),
        }
    }

    /// A plugin's own keyspace: always readable, always writable by it alone.
    pub fn owns(&self, topic: &str) -> bool {
        let (Some(owner), Ok(parsed)) = (self.owner.as_ref(), topic.parse::<Address>()) else {
            return false;
        };
        parsed.owner() == Some(owner.as_str())
    }

    /// Filter the initial snapshot using the same grants as subsequent updates.
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
            topic
                .parse::<Address>()
                .map_err(|error| Refusal::invalid(error.to_string()))?;
            if !self.permits(topic) {
                return Err(Refusal::denied(format!(
                    "topic {topic:?} is not declared by this plugin"
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
