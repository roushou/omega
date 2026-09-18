mod common;
use common::{Harness, expect_refusal, next_result};
use omega_daemon::{
    manifest::ManifestStore,
    storage::{StorageError, Stores},
};
use omega_host::{Layout, TempPath};
use omega_proto::{
    Manifest,
    omega::{storage_request::Operation, *},
};

struct Fixture {
    layout: Layout,
}
impl Fixture {
    fn new() -> Self {
        let root = TempPath::sibling(&std::env::temp_dir().join("omega-storage"), "test");
        Self {
            layout: Layout::at(&root, &root, &root),
        }
    }
    fn descriptor(persistent: bool) -> StorageDescriptor {
        StorageDescriptor {
            id: "example.tasks".into(),
            persistent,
            schema_version: u32::from(persistent),
            codec: "json-v1".into(),
            max_entries: 100,
            max_value_bytes: 1024,
            max_total_bytes: 16 * 1024,
            writable: true,
        }
    }
    fn request(operation: Operation) -> StorageRequest {
        StorageRequest {
            id: "example.tasks".into(),
            operation: Some(operation),
        }
    }
    fn insert(key: &str, value: &str) -> StorageRequest {
        Self::request(Operation::Insert(StorageInsert {
            key: key.into(),
            json: serde_json::to_vec(value).unwrap(),
        }))
    }
    fn read() -> StorageRequest {
        Self::request(Operation::Read(StorageQuery {
            limit: 100,
            ..Default::default()
        }))
    }
    fn replace(key: &str, expected: StorageRevision, value: &str) -> StorageRequest {
        Self::request(Operation::Replace(StorageReplace {
            key: key.into(),
            expected: Some(expected),
            json: serde_json::to_vec(value).unwrap(),
        }))
    }
    fn invoke(stream: u64, op: invoke::Op) -> Frame {
        Frame {
            stream_id: stream,
            body: Some(frame::Body::Invoke(Invoke { op: Some(op) })),
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.layout.state);
    }
}

#[tokio::test]
async fn shared_writers_conflict_and_delete_reinsert_cannot_reuse_a_revision() {
    let fixture = Fixture::new();
    let stores = Stores::default();
    stores
        .prepare(&fixture.layout, vec![Fixture::descriptor(false)])
        .await
        .unwrap();
    let first = stores
        .execute(Fixture::insert("one", "initial"))
        .await
        .unwrap()
        .revision
        .unwrap();
    assert!(matches!(
        stores.execute(Fixture::insert("one", "overwrite")).await,
        Err(StorageError::AlreadyExists)
    ));
    let (a, b) = tokio::join!(
        stores.execute(Fixture::replace("one", first.clone(), "a")),
        stores.execute(Fixture::replace("one", first.clone(), "b"))
    );
    assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1);
    assert!(matches!(
        a.as_ref().err().or(b.as_ref().err()),
        Some(StorageError::Conflict)
    ));
    let current = a.or(b).unwrap().revision.unwrap();
    stores
        .execute(Fixture::request(Operation::Remove(StorageRemove {
            key: "one".into(),
            expected: Some(current.clone()),
        })))
        .await
        .unwrap();
    stores
        .execute(Fixture::insert("one", "again"))
        .await
        .unwrap();
    assert!(matches!(
        stores
            .execute(Fixture::replace("one", current, "stale"))
            .await,
        Err(StorageError::Conflict)
    ));
    let page = stores.execute(Fixture::read()).await.unwrap();
    assert_eq!(page.entries[0].json, br#""again""#);
    assert_eq!(page.revision.unwrap().revision, 4);
}

#[tokio::test]
async fn persistent_restart_preserves_epoch_and_corrupt_files_are_not_replaced() {
    let fixture = Fixture::new();
    let descriptor = Fixture::descriptor(true);
    let stores = Stores::default();
    stores
        .prepare(&fixture.layout, vec![descriptor.clone()])
        .await
        .unwrap();
    let revision = stores
        .execute(Fixture::insert("one", "saved"))
        .await
        .unwrap()
        .revision;
    assert!(omega_host::storage::Lease::acquire(&fixture.layout).is_err());
    drop(stores);
    let stores = Stores::default();
    stores
        .prepare(&fixture.layout, vec![descriptor.clone()])
        .await
        .unwrap();
    let page = stores.execute(Fixture::read()).await.unwrap();
    assert_eq!(page.revision, revision);
    assert_eq!(page.entries[0].json, br#""saved""#);
    drop(stores);
    let path = fixture
        .layout
        .storage_file(&"example.tasks".parse().unwrap());
    std::fs::write(&path, "corrupt").unwrap();
    let stores = Stores::default();
    assert!(
        stores
            .prepare(&fixture.layout, vec![descriptor])
            .await
            .is_err()
    );
    assert_eq!(std::fs::read_to_string(path).unwrap(), "corrupt");
}

#[tokio::test]
async fn rejected_commit_preserves_committed_state_and_allows_recovery() {
    let fixture = Fixture::new();
    let stores = Stores::default();
    stores
        .prepare(&fixture.layout, vec![Fixture::descriptor(true)])
        .await
        .unwrap();
    stores
        .execute(Fixture::insert("one", "saved"))
        .await
        .unwrap();
    let path = fixture
        .layout
        .storage_file(&"example.tasks".parse().unwrap());
    std::fs::remove_file(&path).unwrap();
    std::fs::create_dir(&path).unwrap();
    assert!(matches!(
        stores.execute(Fixture::insert("two", "unsaved")).await,
        Err(StorageError::Unavailable(_))
    ));
    assert_eq!(
        stores.execute(Fixture::read()).await.unwrap().entries.len(),
        1
    );
    std::fs::remove_dir(&path).unwrap();
    stores
        .execute(Fixture::insert("three", "recovered"))
        .await
        .unwrap();
    let info = stores.inspect(None).await.unwrap();
    assert_eq!(info[0]["entries"], 2);
    assert!(info[0]["error"].is_null());
}

#[tokio::test]
async fn limits_and_incompatible_declarations_do_not_replace_committed_data() {
    let fixture = Fixture::new();
    let mut descriptor = Fixture::descriptor(false);
    descriptor.max_entries = 1;
    let stores = Stores::default();
    stores
        .prepare(&fixture.layout, vec![descriptor.clone()])
        .await
        .unwrap();
    stores
        .execute(Fixture::insert("one", "saved"))
        .await
        .unwrap();
    assert!(matches!(
        stores.execute(Fixture::insert("two", "extra")).await,
        Err(StorageError::Exhausted(_))
    ));
    descriptor.persistent = true;
    descriptor.schema_version = 1;
    assert!(
        stores
            .prepare(&fixture.layout, vec![descriptor])
            .await
            .is_err()
    );
    assert_eq!(
        stores.execute(Fixture::read()).await.unwrap().entries.len(),
        1
    );
}

#[tokio::test]
async fn plugin_grants_and_subscription_delivery_use_the_authenticated_session() {
    let fixture = Fixture::new();
    let mut descriptor = Fixture::descriptor(false);
    descriptor.writable = false;
    let mut manifest = Manifest::new(&"reader".parse().unwrap(), "1.0.0");
    manifest.storage.push(descriptor.clone());
    let mut writer = Manifest::new(&"writer".parse().unwrap(), "1.0.0");
    writer.storage.push(Fixture::descriptor(false));
    let harness = Harness::new(
        "storage-auth",
        ManifestStore::from_manifests([manifest.clone(), writer.clone()]),
    );
    harness
        .hub
        .storage()
        .prepare(&fixture.layout, vec![descriptor])
        .await
        .unwrap();
    let token = harness.register_plugin("reader");
    let mut peer = harness.connect(&manifest.hash(), token.as_str()).await;
    peer.recv().await.unwrap().unwrap();
    peer.send(Fixture::invoke(
        1,
        invoke::Op::Storage(Fixture::insert("one", "denied")),
    ))
    .await
    .unwrap();
    assert_eq!(
        expect_refusal(next_result(&mut peer).await).code,
        ErrorCode::PermissionDenied
    );
    peer.send(Fixture::invoke(
        3,
        invoke::Op::StorageSubscribe(StorageSubscribe {
            id: "example.tasks".into(),
            subscription: 7,
            query: Some(StorageQuery {
                limit: 1,
                ..Default::default()
            }),
        }),
    ))
    .await
    .unwrap();
    let mut ack = false;
    let mut snapshot = false;
    while !ack || !snapshot {
        match peer.recv().await.unwrap().unwrap().body.unwrap() {
            frame::Body::Result(result) => {
                assert!(matches!(result.outcome, Some(result::Outcome::Ok(_))));
                ack = true;
            }
            frame::Body::StorageUpdate(update) => {
                assert_eq!(update.subscription, 7);
                assert_eq!(update.page.unwrap().total, 0);
                snapshot = true;
            }
            other => panic!("unexpected {other:?}"),
        }
    }
    let writer_token = harness.register_plugin("writer");
    let mut writing_peer = harness.connect(&writer.hash(), writer_token.as_str()).await;
    writing_peer.recv().await.unwrap().unwrap();
    writing_peer
        .send(Fixture::invoke(
            1,
            invoke::Op::Storage(Fixture::insert("one", "committed")),
        ))
        .await
        .unwrap();
    assert!(matches!(
        common::expect_outcome(next_result(&mut writing_peer).await),
        result::Outcome::Value(_)
    ));
    let frame = tokio::time::timeout(std::time::Duration::from_secs(2), peer.recv())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let Some(frame::Body::StorageUpdate(update)) = frame.body else {
        panic!("missing update")
    };
    assert_eq!(update.page.unwrap().entries[0].json, br#""committed""#);
    peer.send(Fixture::invoke(
        5,
        invoke::Op::StorageInspect(StorageInspect { id: None }),
    ))
    .await
    .unwrap();
    assert_eq!(
        expect_refusal(next_result(&mut peer).await).code,
        ErrorCode::PermissionDenied
    );
    peer.send(Fixture::invoke(
        7,
        invoke::Op::StorageUnsubscribe(StorageUnsubscribe { subscription: 7 }),
    ))
    .await
    .unwrap();
    common::expect_ok(next_result(&mut peer).await);
    harness
        .hub
        .storage()
        .execute(Fixture::insert("two", "later"))
        .await
        .unwrap();
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(30), peer.recv())
            .await
            .is_err()
    );
}
