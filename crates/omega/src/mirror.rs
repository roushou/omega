//! The replicated topics, kept current by the runtime.
//!
//! Internal: an author reaches state through a field like `Battery`, never
//! through this. It exists because something has to hold what the daemon last
//! said, and it should not be the thing an author has to learn.

use std::collections::HashMap;

use omega_proto::omega::{StatePatch, StateSnapshot, StateTopic};
use omega_proto::{SystemTopic, TopicValue};

#[derive(Debug, Default)]
pub(crate) struct Mirror {
    topics: HashMap<String, StateTopic>,
}

impl Mirror {
    pub(crate) fn from_snapshot(snapshot: &StateSnapshot) -> Self {
        let mut mirror = Self::default();
        for topic in &snapshot.topics {
            mirror.insert(topic);
        }
        mirror
    }

    pub(crate) fn apply(&mut self, patch: &StatePatch) {
        for topic in &patch.topics {
            self.insert(topic);
        }
    }

    pub(crate) fn get<T: TopicValue>(&self) -> Option<&T> {
        T::of(self.topics.get(T::TOPIC.as_str())?.value.as_ref()?)
    }

    /// A plugin keyspace's value: generic by nature, because it is whatever
    /// the plugin that owns it says it is.
    pub(crate) fn generic(&self, address: &str) -> Option<&omega_proto::omega::Value> {
        match self.topics.get(address)?.value.as_ref()? {
            omega_proto::omega::state_topic::Value::Generic(value) => Some(value),
            _ => None,
        }
    }

    /// Whether the daemon has spoken about this topic at all.
    ///
    /// Not "has a reading": a topic published with no value is the daemon
    /// saying this machine has none right now — no battery, no adapter, the
    /// broker is down — and that is an answer. Waiting for a value that will
    /// never come is what left a battery widget on a desktop holding its
    /// first render forever.
    pub(crate) fn knows(&self, topic: SystemTopic) -> bool {
        self.topics.contains_key(topic.as_str())
    }

    fn insert(&mut self, topic: &StateTopic) {
        self.topics.insert(topic.topic.clone(), topic.clone());
    }
}
