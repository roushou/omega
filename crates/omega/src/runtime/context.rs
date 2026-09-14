//! Runtime context used by derives to construct dependency handles.
//! Provides replicated state, configuration, and effect admission.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, RwLock};

use omega_proto::omega::{StatePatch, StateSnapshot, invoke};
use omega_proto::{FromValue, SystemTopic, TopicValue, Values};

use crate::runtime::mirror::Mirror;

/// Shared state and effect admission for dependency handles.
#[derive(Clone, Debug)]
pub struct Context {
    instance: Option<omega_proto::instance::InstanceKey>,
    inner: Arc<Shared>,
}

#[derive(Debug)]
struct Shared {
    /// Read on every render, written by the runtime as patches arrive.
    state: RwLock<Mirror>,
    records: Mutex<BTreeMap<String, Values>>,
    effects: crate::effect::queue::EffectsSender,
    tasks: Arc<tokio::sync::Semaphore>,
}

impl Context {
    pub(crate) fn for_instance(&self, instance: omega_proto::instance::InstanceKey) -> Self {
        Self {
            inner: self.inner.clone(),
            instance: Some(instance),
        }
    }
    pub(crate) fn instance(&self) -> Option<omega_proto::instance::InstanceKey> {
        self.instance.clone()
    }
    pub(crate) fn task_budget(&self) -> Arc<tokio::sync::Semaphore> {
        self.inner.tasks.clone()
    }
    pub(crate) fn new(
        snapshot: &StateSnapshot,
        effects: crate::effect::queue::EffectsSender,
    ) -> Self {
        Self {
            instance: None,
            inner: Arc::new(Shared {
                state: RwLock::new(Mirror::from_snapshot(snapshot)),
                records: Mutex::new(BTreeMap::new()),
                effects,
                tasks: Arc::new(tokio::sync::Semaphore::new(256)),
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
