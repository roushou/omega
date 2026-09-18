use crate::storage::{Contract, Revision, Storage};
use omega_proto::omega::{StorageEntry, StoragePage, StorageQuery};
use std::{collections::BTreeMap, marker::PhantomData};

/// An explicit committed snapshot for isolated surface tests and previews.
/// Entries are selected by the surface's actual subscription query. No daemon,
/// filesystem, or automatic effect completion is involved.
///
/// ```
/// use omega::{storage::{Storage, StoragePolicy, Revision}, testing::Stored};
/// struct Tasks;
/// impl Storage for Tasks {
///     type Key = String;
///     type Value = String;
///     const ID: &'static str = "example.tasks";
///     const POLICY: StoragePolicy = StoragePolicy::Memory;
/// }
/// let snapshot = Stored::<Tasks>::new(Revision { epoch: "a".repeat(32), revision: 1 })?
///     .entry("task-1".into(), "Buy coffee".into(), 1)?;
/// # Ok::<(), omega::Error>(())
/// ```
pub struct Stored<S: Storage> {
    revision: Revision,
    entries: BTreeMap<String, StorageEntry>,
    marker: PhantomData<fn() -> S>,
}

impl<S: Storage> std::fmt::Debug for Stored<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Stored")
            .field("id", &S::ID)
            .field("revision", &self.revision)
            .finish()
    }
}

impl<S: Storage> Stored<S> {
    pub fn new(revision: Revision) -> crate::Result<Self> {
        if revision.epoch.len() != 32 || !revision.epoch.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(crate::Error::invalid("fixture epoch needs 32 hex digits"));
        }
        Ok(Self {
            revision,
            entries: BTreeMap::new(),
            marker: PhantomData,
        })
    }

    /// Add one entry. Its revision must be nonzero and no newer than the snapshot.
    pub fn entry(mut self, key: S::Key, value: S::Value, revision: u64) -> crate::Result<Self> {
        let key = key.to_string();
        Contract::<S>::key_text(&key)?;
        let json = serde_json::to_vec(&value).map_err(|e| crate::Error::invalid(e.to_string()))?;
        if revision == 0
            || revision > self.revision.revision
            || json.len() > S::LIMITS.max_value_bytes as usize
            || self.entries.len() >= S::LIMITS.max_entries as usize
            || self.entries.contains_key(&key)
        {
            return Err(crate::Error::invalid(
                "invalid fixture entry revision, key, or size",
            ));
        }
        self.entries.insert(
            key.clone(),
            StorageEntry {
                key,
                json,
                revision,
            },
        );
        Ok(self)
    }

    pub(crate) fn page(&self, query: &StorageQuery) -> StoragePage {
        let mut selected = self
            .entries
            .values()
            .filter(|entry| query.accepts(&entry.key));
        let entries = selected
            .by_ref()
            .take(query.limit as usize)
            .cloned()
            .collect();
        StoragePage {
            revision: Some(self.revision.clone()),
            entries,
            total: self.entries.len() as u64,
            truncated: selected.next().is_some(),
        }
    }
}
