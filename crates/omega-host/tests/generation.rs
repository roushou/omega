use omega_host::{AtomicFile, Generations, Layout, TempPath};
use std::path::PathBuf;

struct Fixture {
    root: PathBuf,
    layout: Layout,
}
impl Fixture {
    fn new() -> Self {
        let root = TempPath::sibling(&std::env::temp_dir().join("omega-generations"), "test");
        let layout = Layout::at(root.join("config"), root.join("state"), root.join("cache"));
        Self { root, layout }
    }
    fn publish(&self, bytes: &[u8]) -> Layout {
        let generation = Generations::new(&self.layout).stage().unwrap();
        generation.files().write("nested/artifact", bytes).unwrap();
        generation.commit().unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[test]
fn resolution_pins_one_complete_generation() {
    let fixture = Fixture::new();
    let first = fixture.publish(b"first");
    let pinned = Generations::new(&fixture.layout)
        .pin_current()
        .unwrap()
        .unwrap();
    let second = fixture.publish(b"second");
    assert_eq!(pinned.layout().state, first.state);
    assert_eq!(
        std::fs::read(pinned.layout().state.join("nested/artifact")).unwrap(),
        b"first"
    );
    assert_eq!(
        Generations::new(&fixture.layout)
            .pin_current()
            .unwrap()
            .unwrap()
            .layout()
            .state,
        second.state
    );
}

#[test]
fn dropping_a_stage_does_not_publish_or_leave_reserved_directories() {
    let fixture = Fixture::new();
    let first = fixture.publish(b"first");
    let generation = Generations::new(&fixture.layout).stage().unwrap();
    generation
        .files()
        .write("nested/artifact", b"abandoned")
        .unwrap();
    drop(generation);
    assert_eq!(
        Generations::new(&fixture.layout)
            .pin_current()
            .unwrap()
            .unwrap()
            .layout()
            .state,
        first.state
    );
    assert_eq!(
        std::fs::read_dir(fixture.layout.generations_dir())
            .unwrap()
            .count(),
        1
    );
}

#[test]
fn references_cannot_escape_the_generation_directory() {
    let fixture = Fixture::new();
    for reference in ["../outside", "/tmp/outside", "..", "one/two", ""] {
        AtomicFile::at(fixture.layout.active_build())
            .write(reference.as_bytes())
            .unwrap();
        assert!(
            Generations::new(&fixture.layout).pin_current().is_err(),
            "{reference:?}"
        );
    }
}

#[test]
fn a_stage_containing_a_symlink_cannot_be_published() {
    let fixture = Fixture::new();
    let first = fixture.publish(b"first");
    let generation = Generations::new(&fixture.layout).stage().unwrap();
    std::os::unix::fs::symlink(&first.state, generation.files().path().join("external")).unwrap();
    assert!(generation.commit().is_err());
    assert_eq!(
        Generations::new(&fixture.layout)
            .pin_current()
            .unwrap()
            .unwrap()
            .layout()
            .state,
        first.state
    );
}

#[test]
fn acceptance_survives_reopening_and_repeated_acceptance_keeps_previous() {
    let fixture = Fixture::new();
    let store = Generations::new(&fixture.layout);
    fixture.publish(b"first");
    let first = store.pin_current().unwrap().unwrap();
    first.accept().unwrap();
    fixture.publish(b"second");
    let second = store.pin_current().unwrap().unwrap();
    second.accept().unwrap();
    second.accept().unwrap();
    assert_eq!(
        Generations::new(&fixture.layout).recovery_ids().unwrap(),
        vec![second.id().clone(), first.id().clone()]
    );
}

#[test]
fn cleanup_protects_references_leases_and_unmanaged_directories() {
    let fixture = Fixture::new();
    let store = Generations::new(&fixture.layout);
    let first = fixture.publish(b"first");
    let pin = store.pin_current().unwrap().unwrap();
    pin.accept().unwrap();
    let second = fixture.publish(b"second");
    store.pin_current().unwrap().unwrap().accept().unwrap();
    let third = fixture.publish(b"third");
    store.pin_current().unwrap().unwrap().accept().unwrap();
    let current = fixture.publish(b"pending");
    let stage = store.stage().unwrap();
    let unmanaged = fixture.layout.generations_dir().join("unmanaged");
    std::fs::create_dir(&unmanaged).unwrap();
    assert!(store.clean().unwrap().is_empty());
    drop(pin);
    assert_eq!(store.clean().unwrap().len(), 1);
    assert!(!first.state.exists());
    for path in [&second.state, &third.state, &current.state, &unmanaged] {
        assert!(path.exists(), "{}", path.display());
    }
    assert!(stage.files().path().exists());
}

#[test]
fn rollback_restores_previous_or_rejected_candidates_acceptance() {
    let fixture = Fixture::new();
    let store = Generations::new(&fixture.layout);
    let first = fixture.publish(b"first");
    store.pin_current().unwrap().unwrap().accept().unwrap();
    let second = fixture.publish(b"second");
    store.pin_current().unwrap().unwrap().accept().unwrap();
    assert_eq!(
        store
            .rollback(None)
            .unwrap()
            .commit()
            .unwrap()
            .layout()
            .state,
        first.state
    );
    // Publishing rollback does not record acceptance until the daemon prepares it.
    assert_eq!(
        store
            .rollback(None)
            .unwrap()
            .commit()
            .unwrap()
            .layout()
            .state,
        second.state
    );
    fixture.publish(b"rejected");
    assert_eq!(
        store
            .rollback(None)
            .unwrap()
            .commit()
            .unwrap()
            .layout()
            .state,
        second.state
    );
    AtomicFile::at(fixture.layout.active_build())
        .write(b"../invalid")
        .unwrap();
    assert_eq!(
        store
            .rollback(None)
            .unwrap()
            .commit()
            .unwrap()
            .layout()
            .state,
        second.state
    );
}

#[test]
fn cleanup_refuses_corrupt_references_before_removing_anything() {
    let fixture = Fixture::new();
    let store = Generations::new(&fixture.layout);
    let unused = fixture.publish(b"unused");
    fixture.publish(b"current");
    let reference = std::fs::read(fixture.layout.active_build()).unwrap();
    AtomicFile::at(fixture.layout.active_build())
        .write(b"../bad")
        .unwrap();
    assert!(store.clean().is_err());
    assert!(unused.state.exists());
    AtomicFile::at(fixture.layout.active_build())
        .write(&reference)
        .unwrap();
    AtomicFile::at(fixture.layout.generation_history())
        .write(b"accepted = '../bad'")
        .unwrap();
    assert!(store.clean().is_err());
    assert!(unused.state.exists());
}

#[test]
fn inherited_lease_survives_parent_release_and_closes_on_child_exit() {
    use std::process::{Command, Stdio};
    let fixture = Fixture::new();
    let store = Generations::new(&fixture.layout);
    let first = fixture.publish(b"first");
    let generation = store.pin_current().unwrap().unwrap();
    let mut command = Command::new("/bin/cat");
    command.stdin(Stdio::piped()).stdout(Stdio::null());
    generation.protect_child(&mut command);
    let mut child = command.spawn().unwrap();
    drop(command);
    drop(generation);
    fixture.publish(b"second");
    let protected = store.clean().unwrap().is_empty() && first.state.exists();
    child.kill().unwrap();
    child.wait().unwrap();
    assert!(
        protected,
        "the child must retain the lease after all parent handles close"
    );
    assert_eq!(store.clean().unwrap().len(), 1);
    assert!(!first.state.exists());
}

#[test]
fn incomplete_directories_cannot_be_pinned_or_selected_for_rollback() {
    let fixture = Fixture::new();
    let store = Generations::new(&fixture.layout);
    let accepted = fixture.publish(b"accepted");
    store.pin_current().unwrap().unwrap().accept().unwrap();
    let reference = std::fs::read(fixture.layout.active_build()).unwrap();
    for marker in ["missing", "directory", "symlink"] {
        let id = omega_host::GenerationId::parse(marker).unwrap();
        let incomplete = Layout::at(
            &fixture.layout.config,
            fixture.layout.generations_dir().join(id.as_str()),
            &fixture.layout.cache,
        );
        std::fs::create_dir(&incomplete.state).unwrap();
        match marker {
            "missing" => {}
            "directory" => std::fs::create_dir(incomplete.generation_ready()).unwrap(),
            "symlink" => std::os::unix::fs::symlink(
                accepted.generation_ready(),
                incomplete.generation_ready(),
            )
            .unwrap(),
            _ => unreachable!(),
        }
        assert!(store.pin(&id).is_err());
        assert!(store.rollback(Some(&id)).is_err());
        assert!(!incomplete.generation_lease().exists());
        AtomicFile::at(fixture.layout.active_build())
            .write(id.as_str().as_bytes())
            .unwrap();
        assert!(store.pin_current().is_err());
        AtomicFile::at(fixture.layout.active_build())
            .write(&reference)
            .unwrap();
        assert!(store.clean().unwrap().is_empty());
        assert!(incomplete.state.exists());
    }
}

#[test]
fn rollback_propagates_current_reference_io_errors_and_can_be_retried() {
    let fixture = Fixture::new();
    let store = Generations::new(&fixture.layout);
    fixture.publish(b"accepted");
    let accepted = store.pin_current().unwrap().unwrap();
    accepted.accept().unwrap();
    let reference = fixture.layout.active_build();
    std::fs::remove_file(&reference).unwrap();
    std::fs::create_dir(&reference).unwrap();
    assert_eq!(
        store.rollback(None).unwrap_err().kind(),
        std::io::ErrorKind::IsADirectory
    );
    std::fs::remove_dir(&reference).unwrap();
    assert_eq!(
        store.rollback(None).unwrap().commit().unwrap().id(),
        accepted.id()
    );
}

#[test]
fn dropping_a_rollback_releases_its_lease_without_changing_references() {
    let fixture = Fixture::new();
    let store = Generations::new(&fixture.layout);
    let candidate = fixture.publish(b"candidate");
    let id = store.pin_current().unwrap().unwrap().id().clone();
    fixture.publish(b"current");
    let reference = std::fs::read(fixture.layout.active_build()).unwrap();
    let rollback = store.rollback(Some(&id)).unwrap();
    assert_eq!(
        std::fs::read(fixture.layout.active_build()).unwrap(),
        reference
    );
    drop(rollback);
    assert_eq!(
        std::fs::read(fixture.layout.active_build()).unwrap(),
        reference
    );
    assert_eq!(store.clean().unwrap(), vec![id]);
    assert!(!candidate.state.exists());
}

#[test]
fn failed_current_write_leaves_a_complete_reclaimable_candidate() {
    let fixture = Fixture::new();
    let store = Generations::new(&fixture.layout);
    fixture.publish(b"accepted");
    let accepted = store.pin_current().unwrap().unwrap();
    accepted.accept().unwrap();
    let reference = std::fs::read(fixture.layout.active_build()).unwrap();
    let stage = store.stage().unwrap();
    stage
        .files()
        .write("nested/artifact", b"candidate")
        .unwrap();
    std::fs::remove_file(fixture.layout.active_build()).unwrap();
    std::fs::create_dir(fixture.layout.active_build()).unwrap();
    assert!(stage.commit().is_err());
    assert!(store.clean().is_err());
    assert_eq!(
        std::fs::read_dir(fixture.layout.generations_dir())
            .unwrap()
            .count(),
        2
    );
    assert_eq!(store.recovery_ids().unwrap(), vec![accepted.id().clone()]);
    std::fs::remove_dir(fixture.layout.active_build()).unwrap();
    AtomicFile::at(fixture.layout.active_build())
        .write(&reference)
        .unwrap();
    assert_eq!(store.clean().unwrap().len(), 1);
    assert_eq!(store.pin_current().unwrap().unwrap().id(), accepted.id());
    fixture.publish(b"retry");
    let retry = store.pin_current().unwrap().unwrap();
    assert_eq!(
        std::fs::read(retry.layout().state.join("nested/artifact")).unwrap(),
        b"retry"
    );
}
