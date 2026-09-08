//! Authoritative state: the daemon owns values and their revisions.

use std::collections::HashMap;

use omega_proto::omega::{StatePatch, StateSnapshot, StateTopic};

/// The daemon's authoritative state. Sources produce *values*; this store
/// assigns the monotonic revisions that drive last-value-wins coalescing.
#[derive(Debug, Default)]
pub struct StateStore {
    topics: HashMap<String, StateTopic>,
    revisions: HashMap<String, u64>,
}

impl StateStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Apply a patch, rebasing each *changed* topic onto its next revision.
    /// Returns the re-versioned patch of what actually changed, which may be
    /// empty.
    ///
    /// A source that polls on an interval republishes the same value over and
    /// over; treating those as updates would bump revisions and wake every
    /// unit for nothing. Last-value-wins means the value, not the poll.
    pub fn apply(&mut self, patch: StatePatch) -> StatePatch {
        let mut changed = Vec::with_capacity(patch.topics.len());

        for StateTopic { topic, value, .. } in patch.topics {
            if self
                .topics
                .get(&topic)
                .is_some_and(|current| current.value == value)
            {
                continue;
            }

            let revision = self.revisions.entry(topic.clone()).or_default();
            *revision += 1;
            let entry = StateTopic {
                topic: topic.clone(),
                revision: *revision,
                value,
            };
            self.topics.insert(topic, entry.clone());
            changed.push(entry);
        }

        StatePatch { topics: changed }
    }

    /// Each named topic the store holds, including one published with no
    /// value — that is the daemon saying there is nothing to report, and
    /// dropping it here would read as never having been asked. Empty
    /// `topics` means every topic.
    pub fn read(&self, topics: &[String]) -> StatePatch {
        let selected = if topics.is_empty() {
            self.topics.values().cloned().collect()
        } else {
            topics
                .iter()
                .filter_map(|topic| self.topics.get(topic).cloned())
                .collect()
        };
        StatePatch { topics: selected }
    }

    /// The full mirror handed to a unit on connect.
    pub fn snapshot(&self) -> StateSnapshot {
        StateSnapshot {
            topics: self.topics.values().cloned().collect(),
        }
    }
}
