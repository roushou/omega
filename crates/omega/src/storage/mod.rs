//! Typed, shared key-value storage declared by ordinary Rust traits.
//!
//! [`Store`] performs asynchronous operations. [`Subscribed`] exposes a bounded
//! local snapshot to rendering. Handle fields declare manifest access; no separate
//! system-document registration is required. Native plugins are not sandboxed.
mod store;
mod subscription;

use omega_proto::omega::{StorageDescriptor, StoragePage, StorageQuery, StorageRevision};
use serde::{Serialize, de::DeserializeOwned};
use std::{fmt::Display, marker::PhantomData, str::FromStr};

pub use store::{ReadOnly, ReadWrite, Store};
pub(crate) use subscription::Subscriptions;
pub use subscription::{Snapshot, Subscribed, Subscription};

/// Persistence implementation for durable stores.
#[derive(Debug, Clone, Copy)]
pub enum Backend {
    Json,
}

/// Memory lasts for the daemon session; persistent JSON survives restart.
#[derive(Debug, Clone, Copy)]
pub enum StoragePolicy {
    Memory,
    Persistent {
        backend: Backend,
        schema_version: u32,
    },
}

/// Encoded data limits, checked by both SDK and daemon.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub max_entries: u32,
    pub max_value_bytes: u32,
    pub max_total_bytes: u32,
}

impl Limits {
    pub const DEFAULT: Self = Self {
        max_entries: StorageDescriptor::MAX_ENTRIES,
        max_value_bytes: StorageDescriptor::MAX_VALUE_BYTES,
        max_total_bytes: StorageDescriptor::MAX_TOTAL_BYTES,
    };
}

/// A shared store contract. IDs survive Rust type, package, and plugin renames.
/// Values use JSON; custom deserializers should enforce domain invariants.
/// Persistent schema version 1 is supported; incompatible files are refused.
///
/// ```
/// use omega::storage::{Storage, StoragePolicy};
/// struct Preferences;
/// impl Storage for Preferences {
///     type Key = String;
///     type Value = String;
///     const ID: &'static str = "example.preferences";
///     const POLICY: StoragePolicy = StoragePolicy::Memory;
/// }
/// ```
pub trait Storage: Send + Sync + 'static {
    type Key: StorageKey;
    type Value: Serialize + DeserializeOwned + Send + Sync + 'static;

    const ID: &'static str;
    const POLICY: StoragePolicy;
    const LIMITS: Limits = Limits::DEFAULT;
}

/// Canonical textual key. Display and parsing must round-trip without changing text.
/// Encoded keys contain 1–256 UTF-8 bytes without control characters.
pub trait StorageKey: Display + FromStr + Send + Sync + 'static {}

impl<T: Display + FromStr + Send + Sync + 'static> StorageKey for T {}

/// Typed key-order query. Limits must be 1–100; enumeration is never unbounded.
#[derive(Debug)]
pub struct Query<S: Storage> {
    pub(crate) wire: StorageQuery,
    marker: PhantomData<fn() -> S>,
}

impl<S: Storage> Default for Query<S> {
    fn default() -> Self {
        Self::new()
    }
}

impl<S: Storage> Query<S> {
    pub fn new() -> Self {
        Self {
            wire: StorageQuery {
                key: None,
                after: None,
                limit: StorageQuery::DEFAULT_LIMIT,
            },
            marker: PhantomData,
        }
    }

    pub fn limit(mut self, limit: u32) -> Self {
        self.wire.limit = limit;
        self
    }

    pub fn after(mut self, key: &S::Key) -> Self {
        self.wire.after = Some(key.to_string());
        self
    }

    pub fn key(key: &S::Key) -> Self {
        Self {
            wire: StorageQuery {
                key: Some(key.to_string()),
                after: None,
                limit: 1,
            },
            marker: PhantomData,
        }
    }

    pub(crate) fn validate(&self) -> crate::Result<()> {
        self.wire
            .validate()
            .map_err(|e| crate::Error::invalid(e.to_string()))?;
        for key in self.wire.key.iter().chain(self.wire.after.iter()) {
            Contract::<S>::key_text(key)?;
        }
        Ok(())
    }
}

/// Compare-and-write token. Epoch prevents reuse across storage resets.
pub type Revision = StorageRevision;
/// One committed entry and its revision token.
#[derive(Debug)]
pub struct Entry<K, V> {
    pub key: K,
    pub value: V,
    pub revision: Revision,
}

/// A consistent bounded result page. `total` counts the entire store.
#[derive(Debug)]
pub struct Page<S: Storage> {
    pub revision: Revision,
    pub total: u64,
    pub truncated: bool,
    entries: Vec<Entry<S::Key, S::Value>>,
}

impl<S: Storage> Page<S> {
    pub fn entries(&self) -> &[Entry<S::Key, S::Value>] {
        &self.entries
    }

    pub(crate) fn decode(page: StoragePage) -> crate::Result<Self> {
        let revision = page
            .revision
            .ok_or_else(|| crate::Error::invalid("storage page has no revision"))?;
        let entries = page
            .entries
            .into_iter()
            .map(|entry| {
                Ok(Entry {
                    key: Contract::<S>::key_text(&entry.key)?,
                    value: serde_json::from_slice(&entry.json)
                        .map_err(|e| crate::Error::invalid(format!("storage {}: {e}", S::ID)))?,
                    revision: Revision {
                        epoch: revision.epoch.clone(),
                        revision: entry.revision,
                    },
                })
            })
            .collect::<crate::Result<_>>()?;
        Ok(Self {
            revision,
            total: page.total,
            truncated: page.truncated,
            entries,
        })
    }

    pub(crate) fn into_entries(self) -> Vec<Entry<S::Key, S::Value>> {
        self.entries
    }
}
pub(crate) struct Contract<S>(PhantomData<S>);
impl<S: Storage> Contract<S> {
    pub(crate) fn descriptor(writable: bool) -> StorageDescriptor {
        let (persistent, schema_version) = match S::POLICY {
            StoragePolicy::Memory => (false, 0),
            StoragePolicy::Persistent { schema_version, .. } => (true, schema_version),
        };
        StorageDescriptor {
            id: S::ID.into(),
            persistent,
            schema_version,
            max_entries: S::LIMITS.max_entries,
            max_value_bytes: S::LIMITS.max_value_bytes,
            max_total_bytes: S::LIMITS.max_total_bytes,
            writable,
            codec: StorageDescriptor::CODEC.into(),
        }
    }

    pub(crate) fn key_text(text: &str) -> crate::Result<S::Key> {
        omega_proto::storage::StorageKey::validate(text)
            .map_err(|e| crate::Error::invalid(e.to_string()))?;
        let key = text
            .parse::<S::Key>()
            .map_err(|_| crate::Error::invalid(format!("invalid key for {}", S::ID)))?;
        if key.to_string() != text {
            return Err(crate::Error::invalid("storage key is not canonical"));
        }
        Ok(key)
    }
}
