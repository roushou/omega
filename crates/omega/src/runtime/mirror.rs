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
        if self
            .topics
            .get(&topic.topic)
            .is_some_and(|current| current.revision >= topic.revision)
        {
            return;
        }
        self.topics.insert(topic.topic.clone(), topic.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use omega_proto::omega::{BatteryState, state_topic};

    #[test]
    fn stale_patches_cannot_undo_a_snapshot_or_a_removal() {
        let topic = StateTopic {
            topic: "battery".into(),
            revision: 10,
            value: Some(state_topic::Value::Battery(BatteryState {
                level: 0.8,
                ..Default::default()
            })),
        };
        let mut mirror = Mirror::from_snapshot(&StateSnapshot {
            topics: vec![topic.clone()],
        });
        mirror.apply(&StatePatch {
            topics: vec![StateTopic {
                revision: 9,
                value: None,
                ..topic.clone()
            }],
        });
        assert_eq!(mirror.get::<BatteryState>().unwrap().level, 0.8);
        mirror.apply(&StatePatch {
            topics: vec![StateTopic {
                revision: 11,
                value: None,
                ..topic.clone()
            }],
        });
        mirror.apply(&StatePatch {
            topics: vec![topic],
        });
        assert!(mirror.get::<BatteryState>().is_none());
        assert!(mirror.knows(SystemTopic::Battery));
    }
}
