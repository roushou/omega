//! Authoritative shared storage. Admission is bounded and mutations serialize per resource.
use omega_host::{
    Layout,
    storage::{Entry, EntryKey, Envelope, JsonStore, Lease},
};
use omega_proto::{
    omega::{StorageDescriptor, StoragePage, StorageRequest, storage_request},
    storage::StorageId,
};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};
use tokio::sync::{Mutex as AsyncMutex, Semaphore, watch};

#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("{0}")]
    Invalid(String),
    #[error("{0}")]
    Incompatible(String),
    #[error("{0}")]
    Exhausted(String),
    #[error("{0}")]
    Unavailable(String),
    #[error("{0}")]
    TooLarge(String),
    #[error("storage outcome unknown: {0}")]
    OutcomeUnknown(String),
    #[error("storage key already exists")]
    AlreadyExists,
    #[error("storage key is missing")]
    NotFound,
    #[error("storage revision conflict")]
    Conflict,
}

#[derive(Debug, Clone)]
pub struct Stores {
    inner: Arc<Inner>,
}

#[derive(Debug)]
struct Inner {
    stores: Mutex<BTreeMap<StorageId, Arc<Resource>>>,
    lease: Mutex<Option<(std::path::PathBuf, Arc<Lease>)>>,
    admission: Arc<Semaphore>,
    finished: tokio::sync::Notify,
    preparation: Arc<AsyncMutex<()>>,
}

#[derive(Debug)]
pub(crate) struct Resource {
    descriptor: StorageDescriptor,
    _lease: Arc<Lease>,
    state: Arc<AsyncMutex<State>>,
    changes: watch::Sender<u64>,
}

#[derive(Debug)]
struct State {
    envelope: Envelope,
    backend: Option<JsonStore>,
    failed: Option<String>,
}

impl Default for Stores {
    fn default() -> Self {
        Self {
            inner: Arc::new(Inner {
                stores: Mutex::new(BTreeMap::new()),
                lease: Mutex::new(None),
                admission: Arc::new(Semaphore::new(32)),
                finished: tokio::sync::Notify::new(),
                preparation: Arc::new(AsyncMutex::new(())),
            }),
        }
    }
}

impl Stores {
    pub(crate) fn close(&self) {
        self.inner.admission.close();
    }

    pub(crate) async fn drain(&self) -> Result<(), StorageError> {
        self.close();
        tokio::time::timeout(std::time::Duration::from_secs(3), async {
            while self.inner.admission.available_permits() != 32 {
                self.inner.finished.notified().await;
            }
        })
        .await
        .map_err(|_| {
            StorageError::OutcomeUnknown(
                "storage shutdown deadline elapsed; admitted writes may still be running".into(),
            )
        })
    }

    pub async fn prepare(
        &self,
        layout: &Layout,
        descriptors: Vec<StorageDescriptor>,
    ) -> Result<(), StorageError> {
        let preparation = self.inner.preparation.clone().lock_owned().await;
        let this = self.clone();
        let layout = layout.clone();
        tokio::task::spawn_blocking(move || {
            let _preparation = preparation;
            if this.inner.admission.is_closed() {
                return Err(StorageError::Unavailable("storage is shutting down".into()));
            }
            let planned = omega_proto::storage::StorageContracts::collect(&descriptors)
                .map_err(|error| StorageError::Incompatible(error.to_string()))?;
            let stores = this
                .inner
                .stores
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .clone();
            if stores.len()
                + planned
                    .keys()
                    .filter(|id| !stores.contains_key(*id))
                    .count()
                > 64
            {
                return Err(StorageError::Exhausted("storage count exceeds 64".into()));
            }
            let mut lease = this.inner.lease.lock().unwrap_or_else(|e| e.into_inner());
            if lease
                .as_ref()
                .is_some_and(|(root, _)| root != &layout.state)
            {
                return Err(StorageError::Incompatible(
                    "storage cannot change its state root".into(),
                ));
            }
            if lease.is_none() && !planned.is_empty() {
                *lease =
                    Some((
                        layout.state.clone(),
                        Arc::new(Lease::acquire(&layout).map_err(|e| {
                            StorageError::Unavailable(format!("storage lease: {e}"))
                        })?),
                    ));
            }
            let mut additions = Vec::new();
            for (id, descriptor) in planned {
                if let Some(existing) = stores.get(&id) {
                    if !existing.descriptor.compatible(&descriptor) {
                        return Err(StorageError::Incompatible(format!(
                            "storage {id} requires explicit migration"
                        )));
                    }
                    continue;
                }
                let backend = descriptor.persistent.then(|| JsonStore::new(&layout, &id));
                let envelope = match &backend {
                    Some(file) => file.load(&descriptor),
                    None => Envelope::empty(&descriptor),
                }
                .map_err(|e| StorageError::Incompatible(format!("storage {id}: {e}")))?;
                let (changes, _) = watch::channel(envelope.revision);
                additions.push((
                    id,
                    Arc::new(Resource {
                        _lease: lease
                            .as_ref()
                            .ok_or_else(|| {
                                StorageError::Unavailable("storage lease is missing".into())
                            })?
                            .1
                            .clone(),
                        descriptor,
                        state: Arc::new(AsyncMutex::new(State {
                            envelope,
                            backend,
                            failed: None,
                        })),
                        changes,
                    }),
                ));
            }
            this.inner
                .stores
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .extend(additions);
            Ok(())
        })
        .await
        .map_err(|e| StorageError::Unavailable(e.to_string()))?
    }

    pub(crate) fn resource(&self, id: &str) -> Result<Arc<Resource>, StorageError> {
        let id: StorageId = id
            .parse()
            .map_err(|e: omega_proto::storage::StorageError| {
                StorageError::Invalid(e.to_string())
            })?;
        self.inner
            .stores
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&id)
            .cloned()
            .ok_or_else(|| StorageError::Incompatible(format!("storage {id} is not initialized")))
    }

    pub async fn execute(&self, request: StorageRequest) -> Result<StoragePage, StorageError> {
        let resource = self.resource(&request.id)?;
        let permit = self
            .inner
            .admission
            .clone()
            .try_acquire_owned()
            .map_err(|_| StorageError::Exhausted("storage operations exhausted".into()))?;
        let admission = Admission {
            permit: Some(permit),
            inner: self.inner.clone(),
        };
        // Once admitted, caller cancellation cannot interrupt a commit.
        tokio::spawn(async move {
            let _admission = admission;
            resource.execute(request).await
        })
        .await
        .map_err(|e| StorageError::OutcomeUnknown(e.to_string()))?
    }

    pub async fn inspect(&self, id: Option<&str>) -> Result<serde_json::Value, StorageError> {
        let resources: Vec<_> = match id {
            Some(id) => vec![self.resource(id)?],
            None => self
                .inner
                .stores
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .values()
                .cloned()
                .collect(),
        };
        let mut values = Vec::new();
        for resource in resources {
            let state = resource.state.lock().await;
            values.push(serde_json::json!({"id":state.envelope.id,"codec":state.envelope.codec,"persistent":resource.descriptor.persistent,"schema_version":state.envelope.schema_version,"epoch":state.envelope.epoch,"revision":state.envelope.revision,"entries":state.envelope.entries.len(),"limits":{"max_entries":resource.descriptor.max_entries,"max_value_bytes":resource.descriptor.max_value_bytes,"max_total_bytes":resource.descriptor.max_total_bytes},"error":state.failed}));
        }
        Ok(serde_json::Value::Array(values))
    }
}
struct Admission {
    permit: Option<tokio::sync::OwnedSemaphorePermit>,
    inner: Arc<Inner>,
}

impl Drop for Admission {
    fn drop(&mut self) {
        drop(self.permit.take());
        self.inner.finished.notify_one();
    }
}

enum Mutation {
    Unchanged(StoragePage),
    Commit { envelope: Envelope, key: EntryKey },
}

impl Resource {
    pub(crate) fn changes(&self) -> watch::Receiver<u64> {
        self.changes.subscribe()
    }

    pub(crate) async fn page(
        &self,
        query: &omega_proto::omega::StorageQuery,
    ) -> Result<StoragePage, StorageError> {
        query
            .validate()
            .map_err(|e| StorageError::Invalid(e.to_string()))?;
        let state = self.state.lock().await;
        if let Some(error) = &state.failed {
            return Err(StorageError::Unavailable(error.clone()));
        }
        Ok(state.envelope.page(query))
    }

    fn plan(
        &self,
        envelope: &Envelope,
        operation: storage_request::Operation,
    ) -> Result<Mutation, StorageError> {
        use storage_request::Operation;
        let mut next = envelope.clone();
        let (key, bytes, expected, insert) = match operation {
            Operation::Insert(op) => (op.key, Some(op.json), None, true),
            Operation::Replace(op) => (
                op.key,
                Some(op.json),
                Some(
                    op.expected
                        .ok_or_else(|| StorageError::Invalid("missing expected revision".into()))?,
                ),
                false,
            ),
            Operation::Remove(op) => (
                op.key,
                None,
                Some(
                    op.expected
                        .ok_or_else(|| StorageError::Invalid("missing expected revision".into()))?,
                ),
                false,
            ),
            Operation::Read(_) => return Err(StorageError::Invalid("expected a mutation".into())),
        };
        let key = EntryKey::try_from(key).map_err(|e| StorageError::Invalid(e.to_string()))?;
        if insert && next.entries.contains_key(&key) {
            return Err(StorageError::AlreadyExists);
        }
        if let Some(expected) = expected {
            let Some(entry) = next.entries.get(&key) else {
                return Err(StorageError::NotFound);
            };
            if expected.epoch != next.epoch || expected.revision != entry.revision {
                return Err(StorageError::Conflict);
            }
        }
        next.revision = next
            .revision
            .checked_add(1)
            .ok_or_else(|| StorageError::Exhausted("storage revision exhausted".into()))?;
        match bytes {
            Some(bytes) => {
                if bytes.len() > self.descriptor.max_value_bytes as usize {
                    return Err(StorageError::TooLarge("storage value exceeds limit".into()));
                }
                let value: serde_json::Value = serde_json::from_slice(&bytes)
                    .map_err(|e| StorageError::Invalid(e.to_string()))?;
                if envelope
                    .entries
                    .get(&key)
                    .is_some_and(|old| old.value == value)
                {
                    return Ok(Mutation::Unchanged(envelope.page(
                        &omega_proto::omega::StorageQuery {
                            key: Some(key.as_str().into()),
                            after: None,
                            limit: 1,
                        },
                    )));
                }
                next.entries.insert(
                    key.clone(),
                    Entry {
                        value,
                        revision: next.revision,
                    },
                );
            }
            None => {
                next.entries.remove(&key);
            }
        }
        next.validate(&self.descriptor)
            .map_err(|e| StorageError::Exhausted(e.to_string()))?;
        Ok(Mutation::Commit {
            envelope: next,
            key,
        })
    }
    async fn execute(&self, request: StorageRequest) -> Result<StoragePage, StorageError> {
        use storage_request::Operation;
        let operation = request
            .operation
            .ok_or_else(|| StorageError::Invalid("missing storage operation".into()))?;
        if let Operation::Read(query) = operation {
            return self.page(&query).await;
        }
        let mut state = self.state.clone().lock_owned().await;
        if let Some(error) = &state.failed {
            return Err(StorageError::Unavailable(error.clone()));
        }
        let (next, key) = match self.plan(&state.envelope, operation)? {
            Mutation::Unchanged(page) => return Ok(page),
            Mutation::Commit { envelope, key } => (envelope, key),
        };
        let changes = self.changes.clone();
        tokio::task::spawn_blocking(move || {
            let outcome = match &state.backend {
                Some(backend) => backend.write(&next),
                None => Ok(()),
            };
            if let Err(error) = state.commit(next, outcome) {
                if state.failed.is_some() {
                    changes.send_modify(|n| *n = n.saturating_add(1));
                }
                return Err(error);
            }
            changes.send_replace(state.envelope.revision);
            Ok(state.envelope.page(&omega_proto::omega::StorageQuery {
                key: Some(key.as_str().into()),
                after: None,
                limit: 1,
            }))
        })
        .await
        .map_err(|e| StorageError::OutcomeUnknown(e.to_string()))?
    }
}

impl State {
    fn commit(
        &mut self,
        next: Envelope,
        outcome: Result<(), omega_host::fs::WriteError>,
    ) -> Result<(), StorageError> {
        match outcome {
            Ok(()) => {
                self.envelope = next;
                Ok(())
            }
            Err(omega_host::fs::WriteError::BeforePublication(error)) => Err(
                StorageError::Unavailable(format!("storage write rejected: {error}")),
            ),
            Err(omega_host::fs::WriteError::AfterPublication(error)) => {
                let reason = format!("restart to reconcile storage durability: {error}");
                self.failed = Some(reason.clone());
                Err(StorageError::OutcomeUnknown(reason))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn caller_cancellation_keeps_admitted_work_and_shutdown_waits_for_it() {
        let root = omega_host::TempPath::sibling(
            &std::env::temp_dir().join("omega-storage-cancellation"),
            "test",
        );
        let layout = Layout::at(&root, &root, &root);
        let descriptor = StorageDescriptor {
            id: "test.data".into(),
            codec: StorageDescriptor::CODEC.into(),
            max_entries: 10,
            max_value_bytes: 1024,
            max_total_bytes: 4096,
            ..Default::default()
        };
        let stores = Stores::default();
        stores.prepare(&layout, vec![descriptor]).await.unwrap();
        let resource = stores.resource("test.data").unwrap();
        let held = resource.state.clone().lock_owned().await;
        let mut write = Box::pin(stores.execute(StorageRequest {
            id: "test.data".into(),
            operation: Some(storage_request::Operation::Insert(
                omega_proto::omega::StorageInsert {
                    key: "one".into(),
                    json: b"1".to_vec(),
                },
            )),
        }));
        assert!(futures_util::poll!(write.as_mut()).is_pending());
        assert_eq!(stores.inner.admission.available_permits(), 31);
        drop(write);
        stores.close();
        assert!(matches!(
            stores
                .execute(StorageRequest {
                    id: "test.data".into(),
                    operation: Some(storage_request::Operation::Read(
                        omega_proto::omega::StorageQuery {
                            limit: 1,
                            ..Default::default()
                        }
                    ))
                })
                .await,
            Err(StorageError::Exhausted(_))
        ));
        let mut drain = Box::pin(stores.drain());
        assert!(futures_util::poll!(drain.as_mut()).is_pending());
        drop(held);
        drain.await.unwrap();
        assert_eq!(
            resource
                .page(&omega_proto::omega::StorageQuery {
                    limit: 1,
                    ..Default::default()
                })
                .await
                .unwrap()
                .entries[0]
                .json,
            b"1"
        );
        drop(stores);
        drop(resource);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn uncertain_publication_suspends_the_resource_without_claiming_a_commit() {
        let descriptor = StorageDescriptor {
            id: "test.data".into(),
            codec: "json-v1".into(),
            max_entries: 10,
            max_value_bytes: 1024,
            max_total_bytes: 4096,
            ..Default::default()
        };
        let envelope = Envelope::empty(&descriptor).unwrap();
        let mut state = State {
            envelope: envelope.clone(),
            backend: None,
            failed: None,
        };
        let mut next = envelope;
        next.revision = 1;
        let result = state.commit(
            next,
            Err(omega_host::fs::WriteError::AfterPublication(
                std::io::Error::from_raw_os_error(libc::EIO),
            )),
        );
        assert!(matches!(result, Err(StorageError::OutcomeUnknown(_))));
        assert!(state.failed.is_some());
        assert_eq!(state.envelope.revision, 0);
    }
}
