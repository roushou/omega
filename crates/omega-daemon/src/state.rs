//! Authoritative state: the daemon owns values and their revisions.

use omega_proto::{Address, AddressError};
use prost::Message;
use std::collections::BTreeMap;

use omega_proto::omega::{StatePatch, StateSnapshot, StateTopic};

/// The daemon's authoritative state. Sources produce *values*; this store
/// assigns the monotonic revisions that drive last-value-wins coalescing.
#[derive(Debug, Default)]
pub struct StateStore {
    topics: BTreeMap<Address, StateTopic>,
    bytes: usize,
    unit_bytes: usize,
    unit_topics: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum StateError {
    #[error(transparent)]
    Address(#[from] AddressError),
    #[error("state payload exceeds publication limit")]
    TooLarge,
    #[error("retained state capacity exhausted")]
    Full,
    #[error("state revision exhausted")]
    RevisionExhausted,
}

impl StateStore {
    pub const BYTE_LIMIT: usize = 2 * 1024 * 1024;
    pub const TOPIC_LIMIT: usize = 4096;
    const DOMAIN_BYTES: usize = Self::BYTE_LIMIT / 2;

    fn size(topic: &StateTopic) -> usize {
        // Reserve the maximum revision width so retraction can always fit.
        topic.encoded_len() + 10
    }

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
    pub fn apply(&mut self, patch: StatePatch) -> Result<StatePatch, StateError> {
        if patch.encoded_len() > Self::BYTE_LIMIT {
            return Err(StateError::TooLarge);
        }
        let mut proposed = BTreeMap::new();
        for topic in patch.topics {
            proposed.insert(Address::parse(&topic.topic)?, topic);
        }
        let mut bytes = self.bytes;
        let mut count = self.unit_topics;
        let mut unit_bytes = self.unit_bytes;
        let mut changed = Vec::new();
        for (address, mut topic) in proposed {
            let is_unit = matches!(address, Address::Unit { .. });
            let current = self.topics.get(&address);
            if current.is_some_and(|current| current.value == topic.value) {
                continue;
            }
            topic.revision = current
                .map_or(0, |current| current.revision)
                .checked_add(1)
                .ok_or(StateError::RevisionExhausted)?;
            let size = Self::size(&topic);
            if size > Self::DOMAIN_BYTES {
                return Err(StateError::TooLarge);
            }
            if let Some(current) = current {
                bytes -= Self::size(current);
                if is_unit {
                    unit_bytes -= Self::size(current);
                }
            } else if is_unit {
                count += 1;
            }
            bytes += size;
            if is_unit {
                unit_bytes += size;
            }
            changed.push((address, topic));
        }
        if unit_bytes > Self::DOMAIN_BYTES
            || bytes - unit_bytes > Self::DOMAIN_BYTES
            || count > Self::TOPIC_LIMIT
        {
            return Err(StateError::Full);
        }
        self.bytes = bytes;
        self.unit_bytes = unit_bytes;
        self.unit_topics = count;
        let mut topics = Vec::with_capacity(changed.len());
        for (address, topic) in changed {
            self.topics.insert(address, topic.clone());
            topics.push(topic);
        }
        Ok(StatePatch { topics })
    }

    /// Each named topic the store holds, including one published with no
    /// value — that is the daemon saying there is nothing to report, and
    /// dropping it here would read as never having been asked. Empty
    /// `topics` means every topic.
    pub fn read(&self, topics: &[String]) -> StatePatch {
        let selected = if topics.is_empty() {
            self.topics.values().cloned().collect()
        } else {
            self.topics
                .values()
                .filter(|entry| topics.contains(&entry.topic))
                .cloned()
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

#[cfg(test)]
mod tests {
    use super::*;
    use omega_proto::{IntoValue, omega::state_topic};

    struct Fixture;
    impl Fixture {
        fn topic(name: &str, size: usize) -> StateTopic {
            StateTopic {
                topic: name.into(),
                revision: 0,
                value: Some(state_topic::Value::Generic("x".repeat(size).into_value())),
            }
        }
        fn patch(name: &str, size: usize) -> StatePatch {
            StatePatch {
                topics: vec![Self::topic(name, size)],
            }
        }
    }

    #[test]
    fn rejected_patches_change_neither_values_nor_revisions() {
        let mut store = StateStore::new();
        store.apply(Fixture::patch("unit.test.a", 600_000)).unwrap();
        let before = store.snapshot();
        let rejected = StatePatch {
            topics: vec![
                Fixture::topic("unit.test.a", 700_000),
                Fixture::topic("unit.test.b", 700_000),
            ],
        };
        assert!(matches!(store.apply(rejected), Err(StateError::Full)));
        assert_eq!(store.snapshot(), before);
        let accepted = store.apply(Fixture::patch("unit.test.a", 1)).unwrap();
        assert_eq!(accepted.topics[0].revision, 2);
        store.apply(Fixture::patch("unit.test.b", 700_000)).unwrap();
    }

    #[test]
    fn unit_capacity_cannot_consume_the_system_reserve() {
        let mut store = StateStore::new();
        store
            .apply(Fixture::patch("unit.test.a", 1_000_000))
            .unwrap();
        assert!(matches!(
            store.apply(Fixture::patch("unit.test.b", 100_000)),
            Err(StateError::Full)
        ));
        store.apply(Fixture::patch("battery", 100_000)).unwrap();
        assert!(store.snapshot().encoded_len() < StateStore::BYTE_LIMIT);
    }

    #[test]
    fn topic_count_is_bounded_even_for_empty_values_and_retractions_remain_ordered() {
        let mut store = StateStore::new();
        for n in 0..StateStore::TOPIC_LIMIT {
            store
                .apply(Fixture::patch(&format!("unit.test.key{n}"), 0))
                .unwrap();
        }
        assert!(matches!(
            store.apply(Fixture::patch("unit.test.extra", 0)),
            Err(StateError::Full)
        ));
        store.apply(Fixture::patch("battery", 0)).unwrap();
        let removed = store
            .apply(StatePatch {
                topics: vec![StateTopic {
                    topic: "unit.test.key0".into(),
                    revision: 0,
                    value: None,
                }],
            })
            .unwrap();
        let recreated = store.apply(Fixture::patch("unit.test.key0", 1)).unwrap();
        assert!(recreated.topics[0].revision > removed.topics[0].revision);
        assert_eq!(store.topics.len(), StateStore::TOPIC_LIMIT + 1);
    }
}
