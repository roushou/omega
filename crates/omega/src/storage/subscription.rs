use super::*;
use crate::{
    runtime::context::Context,
    wiring::{Reads, Wiring},
};
use omega_proto::omega::{StorageSubscribe, StorageUnsubscribe, StorageUpdate, invoke};
use std::{
    collections::BTreeMap,
    sync::{
        Arc, Mutex, Weak,
        atomic::{AtomicU64, Ordering},
    },
};

/// A pure selection of entries from one associated storage resource.
pub trait Subscription: Send + Sync + 'static {
    type Storage: Storage;
    fn query(&self) -> Query<Self::Storage>;
}

/// Subscription state. Loading is distinct from an empty ready page.
pub enum Snapshot<S: Storage> {
    Loading,
    Ready(Arc<Page<S>>),
    Failed(String),
}

#[derive(Debug)]
struct State {
    update: Option<Arc<StorageUpdate>>,
    store: &'static str,
    query: Option<omega_proto::omega::StorageQuery>,
    receipt: Option<crate::effect::Receipt>,
    cleanup: Option<crate::effect::queue::Admission>,
    started: bool,
    instance: Option<omega_proto::instance::InstanceKey>,
}

#[derive(Debug, Default)]
pub(crate) struct Subscriptions {
    states: Mutex<BTreeMap<u64, Weak<Mutex<State>>>>,
}

impl Subscriptions {
    fn register(&self, state: &Arc<Mutex<State>>) -> u64 {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let id = NEXT
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
            .unwrap_or(0);
        self.states
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(id, Arc::downgrade(state));
        id
    }

    pub(crate) fn apply(
        &self,
        update: StorageUpdate,
    ) -> Option<omega_proto::instance::InstanceKey> {
        let states = self.states.lock().unwrap_or_else(|e| e.into_inner());
        let state = states.get(&update.subscription)?.upgrade()?;
        let mut state = state.lock().unwrap_or_else(|e| e.into_inner());
        if let (Some(old), Some(new)) = (
            state
                .update
                .as_ref()
                .and_then(|u| u.page.as_ref())
                .and_then(|p| p.revision.as_ref()),
            update.page.as_ref().and_then(|p| p.revision.as_ref()),
        ) && old.epoch == new.epoch
            && old.revision > new.revision
        {
            return None;
        }
        state.update = Some(Arc::new(update));
        state.instance.clone()
    }

    pub(crate) fn fixture<S: Storage>(
        &self,
        fixture: &crate::testing::Stored<S>,
    ) -> crate::Result<()> {
        let states = self.states.lock().unwrap_or_else(|e| e.into_inner());
        let mut updates = Vec::new();
        for (id, weak) in states.iter() {
            if let Some(state) = weak.upgrade() {
                let state = state.lock().unwrap_or_else(|e| e.into_inner());
                if state.store == S::ID
                    && let Some(query) = &state.query
                {
                    updates.push(StorageUpdate {
                        subscription: *id,
                        page: Some(fixture.page(query)),
                        error: String::new(),
                    });
                }
            }
        }
        drop(states);
        if updates.is_empty() {
            return Err(crate::Error::invalid(
                "surface has no started subscription to this store",
            ));
        }
        for update in updates {
            self.apply(update);
        }
        Ok(())
    }

    pub(crate) fn completions(&self) -> Vec<omega_proto::instance::InstanceKey> {
        let states = self.states.lock().unwrap_or_else(|e| e.into_inner());
        let mut changed = Vec::new();
        for (id, weak) in states.iter() {
            if let Some(state) = weak.upgrade() {
                let mut state = state.lock().unwrap_or_else(|e| e.into_inner());
                if let Some(receipt) = &mut state.receipt
                    && let Some(result) = receipt.try_complete()
                {
                    state.receipt = None;
                    if let Err(error) = result {
                        state.update = Some(Arc::new(StorageUpdate {
                            subscription: *id,
                            page: None,
                            error: error.to_string(),
                        }));
                        if let Some(instance) = &state.instance {
                            changed.push(instance.clone());
                        }
                    }
                }
            }
        }
        changed
    }

    pub(crate) fn close(
        &self,
        instance: Option<&omega_proto::instance::InstanceKey>,
    ) -> crate::Result<()> {
        let mut states = self.states.lock().unwrap_or_else(|e| e.into_inner());
        let mut closed = Vec::new();
        for (id, weak) in states.iter() {
            if let Some(state) = weak.upgrade() {
                let mut state = state.lock().unwrap_or_else(|e| e.into_inner());
                if state.instance.as_ref() == instance {
                    if let Some(cleanup) = state.cleanup.take() {
                        cleanup
                            .submit(invoke::Op::StorageUnsubscribe(StorageUnsubscribe {
                                subscription: *id,
                            }))?
                            .detach();
                    }
                    closed.push(*id);
                }
            }
        }
        for id in closed {
            states.remove(&id);
        }
        Ok(())
    }

    pub(crate) fn validate(
        &self,
        instance: Option<&omega_proto::instance::InstanceKey>,
    ) -> crate::Result<()> {
        let states = self.states.lock().unwrap_or_else(|e| e.into_inner());
        for state in states.values() {
            if let Some(state) = state.upgrade() {
                let state = state.lock().unwrap_or_else(|e| e.into_inner());
                if state.instance.as_ref() == instance && !state.started {
                    return Err(crate::Error::invalid(
                        "subscription was not started in Surface::initialize",
                    ));
                }
            }
        }
        Ok(())
    }
}

/// Instance-owned observation. Start once in `Surface::initialize`; rendering
/// reads snapshots without I/O. Hiding retains observation; destruction ends it.
pub struct Subscribed<Q: Subscription> {
    context: Context,
    id: u64,
    state: Arc<Mutex<State>>,
    cache: Mutex<Option<Cached<Q::Storage>>>,
    marker: PhantomData<fn() -> Q>,
}

impl<Q: Subscription> std::fmt::Debug for Subscribed<Q> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Subscribed").field("id", &self.id).finish()
    }
}

impl<Q: Subscription> Wiring for Subscribed<Q> {
    fn storage() -> Vec<StorageDescriptor> {
        vec![Contract::<Q::Storage>::descriptor(false)]
    }

    fn build(context: &Context) -> Self {
        let state = Arc::new(Mutex::new(State {
            update: None,
            store: Q::Storage::ID,
            query: None,
            receipt: None,
            cleanup: None,
            started: false,
            instance: context.instance(),
        }));
        let id = context.storage().register(&state);
        Self {
            context: context.clone(),
            id,
            state,
            cache: Mutex::new(None),
            marker: PhantomData,
        }
    }
}

impl<Q: Subscription> Reads for Subscribed<Q> {}

impl<Q: Subscription> Subscribed<Q> {
    fn unsubscribe(&self) -> invoke::Op {
        invoke::Op::StorageUnsubscribe(StorageUnsubscribe {
            subscription: self.id,
        })
    }

    /// Validate and admit a subscription without waiting for its initial snapshot.
    pub fn start(&mut self, definition: Q) -> crate::Result<()> {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if self.id == 0 {
            return Err(crate::Error::invalid("storage subscription IDs exhausted"));
        }
        if state.started {
            return Err(crate::Error::invalid("subscription already started"));
        }
        let query = definition.query();
        query.validate()?;
        let cleanup = self
            .context
            .reserve_effect(self.unsubscribe().encoded_len())?;
        state.query = Some(query.wire.clone());
        state.receipt = Some(
            self.context
                .act(invoke::Op::StorageSubscribe(StorageSubscribe {
                    id: Q::Storage::ID.into(),
                    subscription: self.id,
                    query: Some(query.wire),
                }))?,
        );
        state.cleanup = Some(cleanup);
        state.started = true;
        Ok(())
    }

    pub fn snapshot(&self) -> Snapshot<Q::Storage> {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(receipt) = &mut state.receipt
            && let Some(result) = receipt.try_complete()
        {
            state.receipt = None;
            if let Err(error) = result {
                state.update = Some(Arc::new(StorageUpdate {
                    subscription: self.id,
                    page: None,
                    error: error.to_string(),
                }));
            }
        }
        let Some(update) = state.update.clone() else {
            return Snapshot::Loading;
        };
        drop(state);
        let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(cached) = cache.as_ref()
            && Arc::ptr_eq(&cached.update, &update)
        {
            return cached.snapshot.clone();
        }
        let snapshot = if !update.error.is_empty() {
            Snapshot::Failed(update.error.clone())
        } else {
            match update
                .page
                .clone()
                .ok_or_else(|| crate::Error::invalid("missing storage page"))
                .and_then(Page::decode)
            {
                Ok(page) => Snapshot::Ready(Arc::new(page)),
                Err(error) => Snapshot::Failed(error.to_string()),
            }
        };
        *cache = Some(Cached {
            update,
            snapshot: snapshot.clone(),
        });
        snapshot
    }
}

impl<Q: Subscription> Drop for Subscribed<Q> {
    fn drop(&mut self) {
        self.context
            .storage()
            .states
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&self.id);
        if let Some(cleanup) = self
            .state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .cleanup
            .take()
        {
            // Admission reserved the exact cleanup payload when observation started.
            match cleanup.submit(self.unsubscribe()) {
                Ok(receipt) => receipt.detach(),
                Err(error) => {
                    use std::io::Write;
                    let _ = writeln!(
                        std::io::stderr().lock(),
                        "storage cleanup invariant failed: {error}"
                    );
                }
            }
        }
    }
}

impl<S: Storage> std::fmt::Debug for Snapshot<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Loading => f.write_str("Loading"),
            Self::Failed(error) => f.debug_tuple("Failed").field(error).finish(),
            Self::Ready(page) => f
                .debug_struct("Ready")
                .field("revision", &page.revision)
                .field("entries", &page.entries.len())
                .finish(),
        }
    }
}

struct Cached<S: Storage> {
    update: Arc<StorageUpdate>,
    snapshot: Snapshot<S>,
}

impl<S: Storage> Clone for Snapshot<S> {
    fn clone(&self) -> Self {
        match self {
            Self::Loading => Self::Loading,
            Self::Ready(page) => Self::Ready(page.clone()),
            Self::Failed(error) => Self::Failed(error.clone()),
        }
    }
}
