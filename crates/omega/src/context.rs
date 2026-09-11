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

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, RwLock};

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
    records: Mutex<BTreeMap<String, Values>>,
    effects: crate::effect::queue::EffectsSender,
}

impl Context {
    pub(crate) fn new(
        snapshot: &StateSnapshot,
        effects: crate::effect::queue::EffectsSender,
    ) -> Self {
        Self {
            inner: Arc::new(Shared {
                state: RwLock::new(Mirror::from_snapshot(snapshot)),
                records: Mutex::new(BTreeMap::new()),
                effects,
            }),
        }
    }

    pub(crate) fn apply(&self, patch: &StatePatch) {
        self.write().apply(patch);
    }

    /// Whether the daemon has reported each topic, including explicit absence.
    pub(crate) fn holds(&self, topics: &[SystemTopic]) -> bool {
        let state = self.read();
        topics.iter().all(|topic| state.knows(*topic))
    }

    /// The current value of a topic, if the daemon has published one.
    pub fn topic<T: TopicValue + Clone>(&self) -> Option<T> {
        self.read().get::<T>().cloned()
    }

    /// A plugin keyspace, as a map of values.
    pub fn keyspace(&self, address: &str) -> Option<Values> {
        FromValue::from_value(self.read().generic(address)?)
    }

    pub(crate) fn record<T: crate::record::UnitState>(&self) -> T {
        let mut records = self.inner.records.lock().unwrap_or_else(|e| e.into_inner());
        let values = records
            .entry(T::address())
            .or_insert_with(|| self.keyspace(&T::address()).unwrap_or_default());
        T::read(values)
    }

    pub(crate) fn update_record<T: crate::record::UnitState>(
        &self,
        change: impl FnOnce(&mut T),
    ) -> crate::effect::Submission {
        let admission = self.inner.effects.reserve_record()?;
        let mut records = self.inner.records.lock().unwrap_or_else(|e| e.into_inner());
        let values = records
            .entry(T::address())
            .or_insert_with(|| self.keyspace(&T::address()).unwrap_or_default());
        let mut value = T::read(values);
        change(&mut value);
        let updated = value.write();
        let receipt = admission.submit(invoke::Op::SetState(omega_proto::omega::SetState {
            topic: T::address(),
            value: Some(omega_proto::IntoValue::into_value(updated.clone())),
        }))?;
        *values = updated;
        Ok(receipt)
    }

    /// Admit one effect without blocking. Its receipt reports the terminal answer.
    pub fn act(&self, op: invoke::Op) -> crate::effect::Submission {
        self.inner.effects.submit(op)
    }

    pub(crate) fn read(&self) -> std::sync::RwLockReadGuard<'_, Mirror> {
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
