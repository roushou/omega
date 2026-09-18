use super::*;
use crate::{
    runtime::context::Context,
    wiring::{Does, Wiring},
};
use omega_proto::omega::{
    StorageInsert, StorageRemove, StorageReplace, StorageRequest, invoke,
    storage_request::Operation,
};

/// Query-only access for behavior. Rendering uses `Subscribed`.
#[derive(Debug)]
pub struct ReadOnly;
/// Read and conditional-write access; the default for `Store`.
#[derive(Debug)]
pub struct ReadWrite;
mod sealed {
    pub trait Access {
        const WRITE: bool;
    }
}

impl sealed::Access for ReadOnly {
    const WRITE: bool = false;
}

impl sealed::Access for ReadWrite {
    const WRITE: bool = true;
}

/// Asynchronous access to a shared store. Writable by default; use `ReadOnly`
/// for query-only behavior.
///
/// ```compile_fail
/// use omega::storage::{Store, Storage, ReadOnly};
/// async fn write<S: Storage>(store: &Store<S, ReadOnly>, key: S::Key, value: S::Value) {
///     store.insert(key, value).await;
/// }
/// ```
///
/// Rendering uses [`Subscribed`](super::Subscribed).
/// Successful writes are committed; connection failure may leave the outcome
/// unknown. No operation automatically retries a write.
/// Clone the handle to move it into a surface task; clones address the same store.
///
/// ```
/// use omega::{storage::{Store, Storage, Revision}, surface::Task};
/// fn insert<S: Storage>(store: &Store<S>, key: S::Key, value: S::Value)
///     -> Task<omega::Result<Revision>>
/// {
///     let store = store.clone();
///     Task::perform(async move { store.insert(key, value).await }, |result| result)
/// }
/// ```
pub struct Store<S: Storage, A = ReadWrite> {
    context: Context,
    marker: PhantomData<fn() -> (S, A)>,
}

impl<S: Storage, A> Clone for Store<S, A> {
    fn clone(&self) -> Self {
        Self {
            context: self.context.clone(),
            marker: PhantomData,
        }
    }
}

impl<S: Storage, A> std::fmt::Debug for Store<S, A> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Store").field("id", &S::ID).finish()
    }
}

impl<S: Storage, A: sealed::Access + Send + Sync + 'static> Wiring for Store<S, A> {
    fn storage() -> Vec<StorageDescriptor> {
        vec![Contract::<S>::descriptor(A::WRITE)]
    }

    fn build(context: &Context) -> Self {
        Self {
            context: context.clone(),
            marker: PhantomData,
        }
    }
}

impl<S: Storage, A: sealed::Access + Send + Sync + 'static> Does for Store<S, A> {}

impl<S: Storage, A> Store<S, A> {
    async fn request(&self, operation: Operation) -> crate::Result<Page<S>> {
        let response = self
            .context
            .act(invoke::Op::Storage(StorageRequest {
                id: S::ID.into(),
                operation: Some(operation),
            }))?
            .wait()
            .await?
            .ok_or_else(|| crate::Error::invalid("missing storage response"))?;
        Page::decode(
            StoragePage::try_from(&response).map_err(|e| crate::Error::invalid(e.to_string()))?,
        )
    }

    /// Read one committed entry. Absence returns `None`.
    pub async fn get(&self, key: &S::Key) -> crate::Result<Option<Entry<S::Key, S::Value>>> {
        Ok(self
            .list(Query::key(key))
            .await?
            .into_entries()
            .into_iter()
            .next())
    }

    /// Read a bounded page at one committed revision.
    pub async fn list(&self, query: Query<S>) -> crate::Result<Page<S>> {
        query.validate()?;
        self.request(Operation::Read(query.wire)).await
    }
}

impl<S: Storage> Store<S> {
    fn encode(key: &S::Key, value: &S::Value) -> crate::Result<(String, Vec<u8>)> {
        let key = key.to_string();
        Contract::<S>::key_text(&key)?;
        let json = serde_json::to_vec(value).map_err(|e| crate::Error::invalid(e.to_string()))?;
        if json.len() > S::LIMITS.max_value_bytes as usize {
            return Err(omega_proto::Refusal::too_large("storage value exceeds limit").into());
        }
        Ok((key, json))
    }

    /// Create an entry. An existing key is refused rather than overwritten.
    pub async fn insert(&self, key: S::Key, value: S::Value) -> crate::Result<Revision> {
        let (key, json) = Self::encode(&key, &value)?;
        Ok(self
            .request(Operation::Insert(StorageInsert { key, json }))
            .await?
            .into_entries()
            .into_iter()
            .next()
            .ok_or_else(|| crate::Error::invalid("insert returned no entry"))?
            .revision)
    }

    /// Replace only if the entry still has the expected epoch and revision.
    pub async fn replace(
        &self,
        key: S::Key,
        expected: Revision,
        value: S::Value,
    ) -> crate::Result<Revision> {
        let (key, json) = Self::encode(&key, &value)?;
        Ok(self
            .request(Operation::Replace(StorageReplace {
                key,
                json,
                expected: Some(expected),
            }))
            .await?
            .into_entries()
            .into_iter()
            .next()
            .ok_or_else(|| crate::Error::invalid("replace returned no entry"))?
            .revision)
    }

    /// Delete only if the entry still has the expected epoch and revision.
    pub async fn remove(&self, key: S::Key, expected: Revision) -> crate::Result<Revision> {
        let key = key.to_string();
        Contract::<S>::key_text(&key)?;
        Ok(self
            .request(Operation::Remove(StorageRemove {
                key,
                expected: Some(expected),
            }))
            .await?
            .revision)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use omega_proto::IntoValue;
    struct Data;
    impl Storage for Data {
        type Key = String;
        type Value = String;
        const ID: &'static str = "test.data";
        const POLICY: StoragePolicy = StoragePolicy::Memory;
    }
    #[tokio::test]
    async fn unchanged_replace_returns_the_entry_token_instead_of_the_newer_store_revision() {
        let (sender, mut effects) = crate::effect::queue::Effects::channel();
        let context = Context::new(&Default::default(), sender);
        let store = Store::<Data>::build(&context);
        let token = Revision {
            epoch: "a".repeat(32),
            revision: 1,
        };
        let write = store.replace("one".into(), token.clone(), "same".into());
        let reply = async {
            let request = effects.recv().await.unwrap();
            assert!(matches!(request.operation(), invoke::Op::Storage(_)));
            request
                .complete(Ok(Some(
                    StoragePage {
                        revision: Some(Revision {
                            epoch: "a".repeat(32),
                            revision: 9,
                        }),
                        entries: vec![omega_proto::omega::StorageEntry {
                            key: "one".into(),
                            json: br#""same""#.to_vec(),
                            revision: 1,
                        }],
                        total: 2,
                        truncated: false,
                    }
                    .into_value(),
                )))
                .unwrap();
        };
        let (returned, ()) = tokio::join!(write, reply);
        assert_eq!(returned.unwrap(), token);
    }
}
