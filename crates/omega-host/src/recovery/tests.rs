use super::*;
use crate::{AtomicFile, Directory, Layout, TempPath};
use serde::{Deserialize, Serialize};
use std::os::unix::fs::PermissionsExt;
use std::{io, path::PathBuf};

struct Fixture {
    root: PathBuf,
    layout: Layout,
}

impl Fixture {
    fn new() -> Self {
        let root = TempPath::sibling(&std::env::temp_dir().join("omega-recovery"), "test");
        let layout = Layout::at(root.join("config"), root.join("state"), root.join("cache"));
        Directory::create_all(&root).unwrap();
        Self { root, layout }
    }

    fn target(&self) -> PathBuf {
        self.root.join("target")
    }

    fn store(&self) -> RecoveryStore {
        RecoveryStore::new(&self.layout)
    }

    fn replacement(&self) -> Replacement {
        Replacement::prepare(&self.target(), Snapshot::file(b"new".to_vec())).unwrap()
    }

    fn write(&self, bytes: &[u8]) {
        AtomicFile::at(self.target()).write(bytes).unwrap();
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[test]
fn file_installation_is_durable_and_restore_preserves_bytes_and_mode() {
    let f = Fixture::new();
    f.write(b"old");
    std::fs::set_permissions(f.target(), std::fs::Permissions::from_mode(0o600)).unwrap();
    let installed = f.replacement().install(&f.store()).unwrap();
    assert_eq!(installed.outcome, ReplacementOutcome::Updated);
    assert!(installed.recovery.as_ref().unwrap().record.is_file());
    assert_eq!(std::fs::read(f.target()).unwrap(), b"new");
    let mut recovered = f
        .store()
        .open::<Replacement>(&installed.recovery.as_ref().unwrap().receipt.id)
        .unwrap();
    recovered.restore().unwrap();
    recovered.restore().unwrap();
    assert_eq!(std::fs::read(f.target()).unwrap(), b"old");
    assert_eq!(
        std::fs::metadata(f.target()).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert_eq!(recovered.receipt().state, State::Restored);
}

#[test]
fn restoring_a_created_file_removes_it_but_never_later_edits() {
    let f = Fixture::new();
    let installed = f.replacement().install(&f.store()).unwrap();
    assert_eq!(installed.outcome, ReplacementOutcome::Created);
    f.write(b"user edit");
    let mut recovered = f
        .store()
        .open::<Replacement>(&installed.recovery.as_ref().unwrap().receipt.id)
        .unwrap();
    assert!(matches!(recovered.restore(), Err(RecoveryError::Conflict)));
    assert_eq!(std::fs::read(f.target()).unwrap(), b"user edit");
    f.write(b"new");
    recovered.restore().unwrap();
    assert!(!f.target().exists());
}

#[test]
fn interrupted_apply_is_inspected_without_replaying_the_effect() {
    for effect_completed in [false, true] {
        let f = Fixture::new();
        f.write(b"old");
        let mut saved = f.store().prepare(f.replacement()).unwrap();
        let id = saved.receipt().id.clone();
        saved.transition(State::Applying).unwrap();
        if effect_completed {
            saved.record.change.apply().unwrap();
        }
        drop(saved);
        let mut reopened = f.store().open::<Replacement>(&id).unwrap();
        assert_eq!(
            reopened.inspect().unwrap(),
            if effect_completed {
                Observation::After
            } else {
                Observation::Before
            }
        );
        assert!(matches!(reopened.apply(), Err(RecoveryError::InvalidState)));
        if effect_completed {
            reopened.accept().unwrap();
        }
        reopened.restore().unwrap();
        assert_eq!(std::fs::read(f.target()).unwrap(), b"old");
    }
}

#[test]
fn prepared_attempt_blocks_new_changes_until_explicitly_resolved() {
    let f = Fixture::new();
    let saved = f.store().prepare(f.replacement()).unwrap();
    let id = saved.receipt().id.clone();
    drop(saved);
    assert!(matches!(
        f.store().prepare(f.replacement()),
        Err(RecoveryError::Pending(_))
    ));
    f.store()
        .open::<Replacement>(&id)
        .unwrap()
        .restore()
        .unwrap();
    assert!(f.store().prepare(f.replacement()).is_ok());
}

#[test]
fn external_edits_after_preparation_are_not_overwritten() {
    let f = Fixture::new();
    f.write(b"old");
    let mut saved = f.store().prepare(f.replacement()).unwrap();
    f.write(b"external");
    assert!(matches!(saved.apply(), Err(RecoveryError::Conflict)));
    assert_eq!(std::fs::read(f.target()).unwrap(), b"external");
}

#[test]
fn directory_recovery_restores_obsolete_files_and_links_without_following_them() {
    let f = Fixture::new();
    let target = f.target();
    Directory::create_all(&target).unwrap();
    AtomicFile::at(target.join("old.qml"))
        .write(b"old")
        .unwrap();
    let external = f.root.join("external");
    AtomicFile::at(&external).write(b"untouched").unwrap();
    std::os::unix::fs::symlink(&external, target.join("link")).unwrap();
    let desired = f.root.join("desired");
    Directory::create_all(&desired).unwrap();
    AtomicFile::at(desired.join("new.qml"))
        .write(b"new")
        .unwrap();
    let installed = Replacement::prepare(&target, Snapshot::read(&desired).unwrap())
        .unwrap()
        .install(&f.store())
        .unwrap();
    assert!(!target.join("old.qml").exists());
    let mut saved = f
        .store()
        .open::<Replacement>(&installed.recovery.as_ref().unwrap().receipt.id)
        .unwrap();
    saved.restore().unwrap();
    assert_eq!(std::fs::read(target.join("old.qml")).unwrap(), b"old");
    assert_eq!(std::fs::read_link(target.join("link")).unwrap(), external);
    assert_eq!(std::fs::read(&external).unwrap(), b"untouched");
    assert!(!target.join("new.qml").exists());
}

#[test]
fn interrupted_restore_recognizes_already_restored_contents() {
    let f = Fixture::new();
    f.write(b"old");
    let mut saved = f.store().prepare(f.replacement()).unwrap();
    saved.apply().unwrap();
    let id = saved.receipt().id.clone();
    saved.transition(State::Restoring).unwrap();
    saved.record.change.restore().unwrap();
    drop(saved);
    let mut resumed = f.store().open::<Replacement>(&id).unwrap();
    resumed.restore().unwrap();
    assert_eq!(resumed.receipt().state, State::Restored);
}

#[test]
fn invalid_identifiers_records_and_snapshot_paths_are_refused() {
    assert!(ChangeId::parse("../escape").is_err());
    let bad = r#"{"kind":"directory","entries":{"../escape":{"kind":"missing"}},"mode":493}"#;
    assert!(serde_json::from_str::<Snapshot>(bad).is_err());
    let f = Fixture::new();
    let saved = f.store().prepare(f.replacement()).unwrap();
    let id = saved.receipt().id.clone();
    let path = saved.path().to_path_buf();
    drop(saved);
    let mut record: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    record["version"] = 999.into();
    AtomicFile::at(&path)
        .write(&serde_json::to_vec(&record).unwrap())
        .unwrap();
    assert!(matches!(
        f.store().open::<Replacement>(&id),
        Err(RecoveryError::InvalidRecord)
    ));
}

#[test]
fn a_root_symlink_is_refused_without_touching_its_target() {
    let f = Fixture::new();
    let external = f.root.join("external");
    AtomicFile::at(&external).write(b"untouched").unwrap();
    std::os::unix::fs::symlink(&external, f.target()).unwrap();
    assert!(Replacement::prepare(&f.target(), Snapshot::file(b"new".to_vec())).is_err());
    assert_eq!(std::fs::read(external).unwrap(), b"untouched");
}

#[derive(Serialize, Deserialize)]
struct InterruptedEffect {
    replacement: Replacement,
}

impl Change for InterruptedEffect {
    const KIND: &'static str = "interrupted-effect-test";
    type Error = io::Error;

    fn inspect(&self) -> io::Result<Observation> {
        self.replacement.inspect()
    }

    fn apply(&self) -> io::Result<()> {
        self.replacement.apply()?;
        Err(io::Error::other(
            "effect completed but acknowledgment failed",
        ))
    }

    fn restore(&self) -> io::Result<()> {
        self.replacement.restore()
    }

    fn confirm(&self, observation: Observation) -> io::Result<()> {
        self.replacement.confirm(observation)
    }
}

#[test]
fn effect_failure_keeps_intent_and_can_be_recovered_after_reopening() {
    let f = Fixture::new();
    f.write(b"old");
    let mut saved = f
        .store()
        .prepare(InterruptedEffect {
            replacement: f.replacement(),
        })
        .unwrap();
    let id = saved.receipt().id.clone();
    assert!(matches!(saved.apply(), Err(RecoveryError::Recorded { .. })));
    assert_eq!(saved.receipt().state, State::Applying);
    drop(saved);
    let mut reopened = f.store().open::<InterruptedEffect>(&id).unwrap();
    assert_eq!(reopened.inspect().unwrap(), Observation::After);
    reopened.restore().unwrap();
    assert_eq!(std::fs::read(f.target()).unwrap(), b"old");
}

#[test]
fn a_failed_intent_write_prevents_the_effect() {
    let f = Fixture::new();
    f.write(b"old");
    let mut saved = f.store().prepare(f.replacement()).unwrap();
    let record = saved.path().to_path_buf();
    std::fs::remove_file(&record).unwrap();
    std::fs::create_dir(&record).unwrap();
    assert!(saved.apply().is_err());
    assert_eq!(std::fs::read(f.target()).unwrap(), b"old");
}

#[test]
fn directory_external_edits_block_recovery_and_survive() {
    let f = Fixture::new();
    let desired = f.root.join("desired");
    Directory::create_all(&desired).unwrap();
    AtomicFile::at(desired.join("file")).write(b"new").unwrap();
    let installed = Replacement::prepare(&f.target(), Snapshot::read(&desired).unwrap())
        .unwrap()
        .install(&f.store())
        .unwrap();
    AtomicFile::at(f.target().join("user-file"))
        .write(b"keep")
        .unwrap();
    let mut saved = f
        .store()
        .open::<Replacement>(&installed.recovery.as_ref().unwrap().receipt.id)
        .unwrap();
    assert!(matches!(saved.restore(), Err(RecoveryError::Conflict)));
    assert_eq!(
        std::fs::read(f.target().join("user-file")).unwrap(),
        b"keep"
    );
}

#[test]
fn reinstalling_identical_files_does_not_create_recovery_records() {
    let f = Fixture::new();
    f.write(b"new");
    assert_eq!(f.replacement().inspect().unwrap(), Observation::Unchanged);
    let installed = f.replacement().install(&f.store()).unwrap();
    assert_eq!(installed.outcome, ReplacementOutcome::Unchanged);
    assert!(installed.recovery.is_none());
    assert!(!f.layout.recovery_dir().exists());

    f.write(b"old");
    let first = f.replacement().install(&f.store()).unwrap();
    assert!(first.recovery.is_some());
    let second = f.replacement().install(&f.store()).unwrap();
    assert!(second.recovery.is_none());
    assert_eq!(f.store().receipts().unwrap().len(), 1);
}

#[test]
fn prepared_and_restored_records_cannot_undo_later_matching_edits() {
    let f = Fixture::new();
    f.write(b"old");
    let mut saved = f.store().prepare(f.replacement()).unwrap();
    f.write(b"new");
    assert!(matches!(saved.restore(), Err(RecoveryError::Conflict)));
    assert_eq!(std::fs::read(f.target()).unwrap(), b"new");
    f.write(b"old");
    saved.apply().unwrap();
    saved.restore().unwrap();
    f.write(b"new");
    assert!(matches!(saved.restore(), Err(RecoveryError::Conflict)));
    assert_eq!(std::fs::read(f.target()).unwrap(), b"new");
}

#[test]
fn failed_journal_writes_require_reopening_before_any_more_effects() {
    let f = Fixture::new();
    f.write(b"old");
    let mut saved = f.store().prepare(f.replacement()).unwrap();
    let path = saved.path().to_path_buf();
    let original_record = std::fs::read(&path).unwrap();
    std::fs::remove_file(&path).unwrap();
    std::fs::create_dir(&path).unwrap();
    assert!(saved.apply().is_err());
    assert_eq!(saved.receipt().state, State::Prepared);
    assert!(matches!(
        saved.apply(),
        Err(RecoveryError::ReopenRequired(_))
    ));
    assert!(matches!(
        saved.accept(),
        Err(RecoveryError::ReopenRequired(_))
    ));
    assert!(matches!(
        saved.restore(),
        Err(RecoveryError::ReopenRequired(_))
    ));
    assert_eq!(std::fs::read(f.target()).unwrap(), b"old");
    let id = saved.receipt().id.clone();
    drop(saved);
    std::fs::remove_dir(&path).unwrap();
    AtomicFile::at(path).write(&original_record).unwrap();
    f.store().open::<Replacement>(&id).unwrap().apply().unwrap();
    assert_eq!(std::fs::read(f.target()).unwrap(), b"new");
}

#[derive(Serialize, Deserialize)]
struct BrokenBackend {
    replacement: Replacement,
}

impl Change for BrokenBackend {
    const KIND: &'static str = "broken-backend-test";
    type Error = io::Error;

    fn inspect(&self) -> io::Result<Observation> {
        self.replacement.inspect()
    }

    fn apply(&self) -> io::Result<()> {
        Ok(())
    }

    fn restore(&self) -> io::Result<()> {
        Ok(())
    }

    fn confirm(&self, _: Observation) -> io::Result<()> {
        Err(io::Error::other("flush failed"))
    }
}

#[test]
fn backend_success_without_the_expected_effect_is_not_committed() {
    let f = Fixture::new();
    f.write(b"old");
    let mut saved = f
        .store()
        .prepare(BrokenBackend {
            replacement: f.replacement(),
        })
        .unwrap();
    assert!(
        matches!(saved.apply(), Err(RecoveryError::Recorded { source, .. }) if matches!(*source, RecoveryError::Postcondition))
    );
    assert_eq!(saved.receipt().state, State::Applying);
    f.write(b"new");
    assert!(saved.restore().is_err());
    assert_eq!(saved.receipt().state, State::Restoring);
    assert_eq!(std::fs::read(f.target()).unwrap(), b"new");
}

#[test]
fn accepting_an_observed_effect_requires_durability_confirmation() {
    let f = Fixture::new();
    f.write(b"old");
    let mut saved = f
        .store()
        .prepare(BrokenBackend {
            replacement: f.replacement(),
        })
        .unwrap();
    saved.transition(State::Applying).unwrap();
    f.write(b"new");
    assert!(saved.accept().is_err());
    assert_eq!(saved.receipt().state, State::Applying);
}

#[tokio::test]
async fn backup_records_are_private_and_store_lock_is_held_until_handle_drop() {
    let f = Fixture::new();
    let saved = f.store().prepare(f.replacement()).unwrap();
    assert_eq!(
        std::fs::metadata(saved.path())
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    assert_eq!(
        std::fs::metadata(f.layout.recovery_dir())
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    assert!(
        crate::fs::FileLock::try_exclusive(&f.layout.recovery_lock())
            .unwrap()
            .is_none()
    );
    drop(saved);

    // Concurrent subprocess tests can inherit the descriptor until exec closes it.
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            if crate::fs::FileLock::try_exclusive(&f.layout.recovery_lock())
                .unwrap()
                .is_some()
            {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(1)).await;
        }
    })
    .await
    .expect("dropping the recovery handle must release its lock");
}

#[test]
fn a_journal_error_after_publication_requires_reloading_the_actual_state() {
    let f = Fixture::new();
    f.write(b"old");
    let mut saved = f.store().prepare(f.replacement()).unwrap();
    let id = saved.receipt().id.clone();
    saved.fail_after_save = Some(State::Applied);
    assert!(saved.apply().is_err());
    assert_eq!(saved.receipt().state, State::Applying);
    assert!(matches!(
        saved.accept(),
        Err(RecoveryError::ReopenRequired(_))
    ));
    assert!(matches!(
        saved.restore(),
        Err(RecoveryError::ReopenRequired(_))
    ));
    assert_eq!(std::fs::read(f.target()).unwrap(), b"new");
    drop(saved);
    let mut reopened = f.store().open::<Replacement>(&id).unwrap();
    assert_eq!(reopened.receipt().state, State::Applied);
    reopened.restore().unwrap();
    assert_eq!(std::fs::read(f.target()).unwrap(), b"old");
}

#[test]
fn listing_requires_a_payload_and_refuses_symlink_records() {
    let f = Fixture::new();
    let saved = f.store().prepare(f.replacement()).unwrap();
    let path = saved.path().to_path_buf();
    let mut record: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    drop(saved);
    record.as_object_mut().unwrap().remove("change");
    AtomicFile::at(&path)
        .write(&serde_json::to_vec(&record).unwrap())
        .unwrap();
    assert!(f.store().receipts().is_err());
    std::fs::remove_file(&path).unwrap();
    std::os::unix::fs::symlink(f.root.join("external"), &path).unwrap();
    assert!(f.store().receipts().is_err());
}
