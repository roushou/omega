//! What a plugin's fields are built from.
//!
//! A plugin declares what it needs by *holding* it: a `Battery` field is a
//! handle onto the replicated battery topic, a `Notify` field is permission
//! to raise a notification. Both are built from the same [`Context`] — the
//! live state the runtime keeps current, and the queue effects leave on their
//! way to the daemon.
//!
//! Authors never see this type. It exists so the derive has something to
//! build fields out of.

use std::sync::{Arc, RwLock};

use omega_proto::omega::{StatePatch, StateSnapshot, invoke};
use omega_proto::{FromValue, SystemTopic, TopicValue, Values};

use crate::mirror::Mirror;

/// The runtime, as a field sees it.
#[derive(Clone, Debug)]
pub struct Context {
    inner: Arc<Shared>,
}

#[derive(Debug)]
struct Shared {
    /// Read on every render, written by the runtime as patches arrive.
    state: RwLock<Mirror>,
    /// Where effects go. Unbounded because a plugin must never block on the
    /// daemon draining it — a wedged socket is the runtime's problem, not the
    /// author's.
    effects: tokio::sync::mpsc::UnboundedSender<invoke::Op>,
}

impl Context {
    pub(crate) fn new(
        snapshot: &StateSnapshot,
        effects: tokio::sync::mpsc::UnboundedSender<invoke::Op>,
    ) -> Self {
        Self {
            inner: Arc::new(Shared {
                state: RwLock::new(Mirror::from_snapshot(snapshot)),
                effects,
            }),
        }
    }

    pub(crate) fn apply(&self, patch: &StatePatch) {
        self.write().apply(patch);
    }

    /// Whether every one of these topics has a value yet.
    ///
    /// The runtime holds a plugin's first render until they do, so a field
    /// can read its topic without asking whether it exists.
    pub(crate) fn holds(&self, topics: &[SystemTopic]) -> bool {
        let state = self.read();
        topics.iter().all(|topic| state.has(*topic))
    }

    /// The current value of a topic, if the daemon has published one.
    pub fn topic<T: TopicValue + Clone>(&self) -> Option<T> {
        self.read().get::<T>().cloned()
    }

    /// A plugin keyspace, as a map of values.
    pub fn keyspace(&self, address: &str) -> Option<Values> {
        FromValue::from_value(self.read().generic(address)?)
    }

    /// Queue an effect. Fire and forget: the daemon answers, and a refusal
    /// surfaces in the runtime's log rather than at an author's call site
    /// that had nothing to do about it.
    pub fn act(&self, op: invoke::Op) {
        let _ = self.inner.effects.send(op);
    }

    fn read(&self) -> std::sync::RwLockReadGuard<'_, Mirror> {
        self.inner
            .state
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn write(&self) -> std::sync::RwLockWriteGuard<'_, Mirror> {
        self.inner
            .state
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}
