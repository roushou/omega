use super::{FixtureRevision, Stored};
use crate::{
    effect::EffectError,
    storage::{Contract, Revision, Storage},
    testing::CapturedEffect,
};
use omega_proto::{
    IntoValue, Refusal,
    omega::{
        StorageEntry, StoragePage, StorageQuery as WireQuery, invoke, storage_request::Operation,
    },
};

/// The typed selection captured from a storage read.
/// Keys and cursors use the store's key type; limits count entries.
#[derive(Debug)]
pub struct StorageQuery<K> {
    key: Option<K>,
    after: Option<K>,
    limit: u32,
}

impl<K> StorageQuery<K> {
    /// The exact key requested, or `None` for enumeration.
    pub fn key(&self) -> Option<&K> {
        self.key.as_ref()
    }
    /// The exclusive cursor, or `None` to start at the first key.
    pub fn after(&self) -> Option<&K> {
        self.after.as_ref()
    }
    /// Maximum entries returned by this read.
    pub fn limit(&self) -> u32 {
        self.limit
    }
}

struct Reply(CapturedEffect);

impl Reply {
    fn operation<'a, S: Storage>(
        capture: &'a CapturedEffect,
        expected: &'static str,
    ) -> crate::Result<&'a Operation> {
        let invoke::Op::Storage(request) = capture.operation() else {
            return Err(crate::Error::invalid(format!(
                "expected storage {expected} for {}, got a non-storage effect",
                S::ID
            )));
        };
        if request.id != S::ID {
            return Err(crate::Error::invalid(format!(
                "expected storage {}, got {}",
                S::ID,
                request.id
            )));
        }
        let operation = request
            .operation
            .as_ref()
            .ok_or_else(|| crate::Error::invalid("storage request has no operation"))?;
        Ok(operation)
    }

    fn unexpected<S: Storage>(expected: &str, operation: &Operation) -> crate::Error {
        let actual = match operation {
            Operation::Read(_) => "read",
            Operation::Insert(_) => "insert",
            Operation::Replace(_) => "replace",
            Operation::Remove(_) => "remove",
        };
        crate::Error::invalid(format!(
            "expected storage {expected} for {}, got {actual}",
            S::ID
        ))
    }

    fn value<S: Storage>(json: &[u8]) -> crate::Result<S::Value> {
        if json.len() > S::LIMITS.max_value_bytes as usize {
            return Err(crate::Error::invalid(
                "captured storage value exceeds the store's limit",
            ));
        }
        serde_json::from_slice(json).map_err(|error| {
            crate::Error::invalid(format!("invalid captured storage value: {error}"))
        })
    }

    fn expected(revision: &Option<Revision>) -> crate::Result<Revision> {
        let revision = revision.clone().ok_or_else(|| {
            crate::Error::invalid("captured storage write has no expected revision")
        })?;
        FixtureRevision::entry(&revision)?;
        Ok(revision)
    }

    fn write_revision(revision: &Revision, expected: &Revision, remove: bool) -> crate::Result<()> {
        FixtureRevision::entry(revision)?;
        if revision.epoch != expected.epoch
            || revision.revision < expected.revision
            || (remove && revision.revision == expected.revision)
        {
            return Err(crate::Error::invalid(
                "storage success requires the same epoch and an advancing revision (unchanged replacements may retain their revision)",
            ));
        }
        Ok(())
    }

    fn page(self, page: StoragePage) -> crate::Result<()> {
        Ok(self.0.complete(Ok(Some(page.into_value())))?)
    }

    fn written(self, key: String, json: Vec<u8>, revision: Revision) -> crate::Result<()> {
        FixtureRevision::entry(&revision)?;
        self.page(StoragePage {
            entries: vec![StorageEntry {
                key,
                json,
                revision: revision.revision,
            }],
            revision: Some(revision),
            // Write receipts are not snapshots of the store's other entries.
            total: 1,
            truncated: false,
        })
    }
}

/// A captured read awaiting an explicit snapshot or failure.
/// `reply` applies the captured key, cursor and limit to a [`Stored`] fixture.
/// Dropping the capture closes its receipt; no backend is contacted.
#[must_use = "complete the captured request explicitly or drop it to simulate disconnection"]
pub struct StorageRead<S: Storage> {
    reply: Reply,
    query: StorageQuery<S::Key>,
    wire: WireQuery,
}

impl<S: Storage> TryFrom<CapturedEffect> for StorageRead<S> {
    type Error = crate::Error;
    fn try_from(capture: CapturedEffect) -> crate::Result<Self> {
        let operation = Reply::operation::<S>(&capture, "read")?;
        let Operation::Read(wire) = operation else {
            return Err(Reply::unexpected::<S>("read", operation));
        };
        wire.validate()
            .map_err(|error| crate::Error::invalid(error.to_string()))?;
        let query = StorageQuery {
            key: wire
                .key
                .as_deref()
                .map(Contract::<S>::key_text)
                .transpose()?,
            after: wire
                .after
                .as_deref()
                .map(Contract::<S>::key_text)
                .transpose()?,
            limit: wire.limit,
        };
        let wire = wire.clone();
        Ok(Self {
            reply: Reply(capture),
            query,
            wire,
        })
    }
}

impl<S: Storage> StorageRead<S> {
    /// Inspect the captured selection without decoding protocol fields.
    pub fn query(&self) -> &StorageQuery<S::Key> {
        &self.query
    }

    /// Complete this read with a bounded page selected from the fixture.
    /// This does not publish a subscription update or modify the fixture.
    pub fn reply(self, snapshot: &Stored<S>) -> crate::Result<()> {
        self.reply.page(snapshot.page(&self.wire))
    }
}

/// A captured insert awaiting an explicit success or failure.
/// Inspect its typed inputs before completing it. Dropping it closes its receipt.
/// Success only acknowledges this request; subscription delivery stays explicit.
#[must_use = "complete the captured request explicitly or drop it to simulate disconnection"]
pub struct StorageInsert<S: Storage> {
    reply: Reply,
    key: S::Key,
    value: S::Value,
    json: Vec<u8>,
}

impl<S: Storage> TryFrom<CapturedEffect> for StorageInsert<S> {
    type Error = crate::Error;
    fn try_from(capture: CapturedEffect) -> crate::Result<Self> {
        let operation = Reply::operation::<S>(&capture, "insert")?;
        let Operation::Insert(request) = operation else {
            return Err(Reply::unexpected::<S>("insert", operation));
        };
        let key = Contract::<S>::key_text(&request.key)?;
        let value = Reply::value::<S>(&request.json)?;
        let json = request.json.clone();
        Ok(Self {
            reply: Reply(capture),
            key,
            value,
            json,
        })
    }
}

impl<S: Storage> StorageInsert<S> {
    /// The application-defined key being changed.
    pub fn key(&self) -> &S::Key {
        &self.key
    }
    /// The decoded value submitted by the plugin.
    pub fn value(&self) -> &S::Value {
        &self.value
    }
    /// Acknowledge success without writing data or notifying subscriptions.
    /// The supplied revision is the committed entry token and must be nonzero.
    /// Invalid tokens return an error and close the isolated receipt.
    pub fn succeed(self, revision: Revision) -> crate::Result<()> {
        self.reply
            .written(self.key.to_string(), self.json, revision)
    }
}

/// A captured replace awaiting an explicit success or failure.
/// Inspect its typed inputs before completing it. Dropping it closes its receipt.
/// Success only acknowledges this request; subscription delivery stays explicit.
#[must_use = "complete the captured request explicitly or drop it to simulate disconnection"]
pub struct StorageReplace<S: Storage> {
    reply: Reply,
    key: S::Key,
    value: S::Value,
    json: Vec<u8>,
    expected: Revision,
}

impl<S: Storage> TryFrom<CapturedEffect> for StorageReplace<S> {
    type Error = crate::Error;
    fn try_from(capture: CapturedEffect) -> crate::Result<Self> {
        let operation = Reply::operation::<S>(&capture, "replace")?;
        let Operation::Replace(request) = operation else {
            return Err(Reply::unexpected::<S>("replace", operation));
        };
        let key = Contract::<S>::key_text(&request.key)?;
        let value = Reply::value::<S>(&request.json)?;
        let json = request.json.clone();
        let expected = Reply::expected(&request.expected)?;
        Ok(Self {
            reply: Reply(capture),
            key,
            value,
            json,
            expected,
        })
    }
}

impl<S: Storage> StorageReplace<S> {
    /// The application-defined key being changed.
    pub fn key(&self) -> &S::Key {
        &self.key
    }
    /// The decoded value submitted by the plugin.
    pub fn value(&self) -> &S::Value {
        &self.value
    }
    /// The entry revision the plugin expects to change.
    pub fn expected(&self) -> &Revision {
        &self.expected
    }
    /// Acknowledge success without writing data or notifying subscriptions.
    /// Use the committed entry token, which may equal `expected()` for an unchanged value.
    /// Its epoch must match and its revision must not go backwards.
    /// Invalid tokens return an error and close the isolated receipt.
    pub fn succeed(self, revision: Revision) -> crate::Result<()> {
        Reply::write_revision(&revision, &self.expected, false)?;
        self.reply
            .written(self.key.to_string(), self.json, revision)
    }
}

/// A captured remove awaiting an explicit success or failure.
/// Inspect its typed inputs before completing it. Dropping it closes its receipt.
/// Success only acknowledges this request; subscription delivery stays explicit.
#[must_use = "complete the captured request explicitly or drop it to simulate disconnection"]
pub struct StorageRemove<S: Storage> {
    reply: Reply,
    key: S::Key,
    expected: Revision,
}

impl<S: Storage> TryFrom<CapturedEffect> for StorageRemove<S> {
    type Error = crate::Error;
    fn try_from(capture: CapturedEffect) -> crate::Result<Self> {
        let operation = Reply::operation::<S>(&capture, "remove")?;
        let Operation::Remove(request) = operation else {
            return Err(Reply::unexpected::<S>("remove", operation));
        };
        let key = Contract::<S>::key_text(&request.key)?;
        let expected = Reply::expected(&request.expected)?;
        Ok(Self {
            reply: Reply(capture),
            key,
            expected,
        })
    }
}

impl<S: Storage> StorageRemove<S> {
    /// The application-defined key being changed.
    pub fn key(&self) -> &S::Key {
        &self.key
    }
    /// The entry revision the plugin expects to change.
    pub fn expected(&self) -> &Revision {
        &self.expected
    }
    /// Acknowledge success without writing data or notifying subscriptions.
    /// Use the new store revision: its epoch must match and its revision must advance.
    /// Invalid tokens return an error and close the isolated receipt.
    pub fn succeed(self, revision: Revision) -> crate::Result<()> {
        Reply::write_revision(&revision, &self.expected, true)?;
        self.reply.page(StoragePage {
            revision: Some(revision),
            ..Default::default()
        })
    }
}

impl<S: Storage> StorageRead<S> {
    /// Complete with a daemon refusal, preserving its code and message.
    pub fn refuse(self, refusal: Refusal) -> crate::Result<()> {
        self.fail(EffectError::Refused(refusal))
    }

    /// Complete with an explicit effect failure, without contacting a backend.
    pub fn fail(self, error: EffectError) -> crate::Result<()> {
        Ok(self.reply.0.complete(Err(error))?)
    }
}

impl<S: Storage> std::fmt::Debug for StorageRead<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StorageRead")
            .field("store", &S::ID)
            .finish_non_exhaustive()
    }
}

impl<S: Storage> StorageInsert<S> {
    /// Complete with a daemon refusal, preserving its code and message.
    pub fn refuse(self, refusal: Refusal) -> crate::Result<()> {
        self.fail(EffectError::Refused(refusal))
    }

    /// Complete with an explicit effect failure, without contacting a backend.
    pub fn fail(self, error: EffectError) -> crate::Result<()> {
        Ok(self.reply.0.complete(Err(error))?)
    }
}

impl<S: Storage> std::fmt::Debug for StorageInsert<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StorageInsert")
            .field("store", &S::ID)
            .finish_non_exhaustive()
    }
}

impl<S: Storage> StorageReplace<S> {
    /// Complete with a daemon refusal, preserving its code and message.
    pub fn refuse(self, refusal: Refusal) -> crate::Result<()> {
        self.fail(EffectError::Refused(refusal))
    }

    /// Complete with an explicit effect failure, without contacting a backend.
    pub fn fail(self, error: EffectError) -> crate::Result<()> {
        Ok(self.reply.0.complete(Err(error))?)
    }
}

impl<S: Storage> std::fmt::Debug for StorageReplace<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StorageReplace")
            .field("store", &S::ID)
            .finish_non_exhaustive()
    }
}

impl<S: Storage> StorageRemove<S> {
    /// Complete with a daemon refusal, preserving its code and message.
    pub fn refuse(self, refusal: Refusal) -> crate::Result<()> {
        self.fail(EffectError::Refused(refusal))
    }

    /// Complete with an explicit effect failure, without contacting a backend.
    pub fn fail(self, error: EffectError) -> crate::Result<()> {
        Ok(self.reply.0.complete(Err(error))?)
    }
}

impl<S: Storage> std::fmt::Debug for StorageRemove<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StorageRemove")
            .field("store", &S::ID)
            .finish_non_exhaustive()
    }
}
