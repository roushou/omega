use crate::{AtomicFile, Directory, Generations, Layout, TempPath};
use std::{path::PathBuf, process::Command};

struct Fixture {
    root: PathBuf,
    layout: Layout,
}

impl Fixture {
    fn new() -> Self {
        Self::at(TempPath::sibling(
            &std::env::temp_dir().join("omega-crash"),
            "test",
        ))
    }

    fn at(root: PathBuf) -> Self {
        let layout = Layout::at(root.join("config"), root.join("state"), root.join("cache"));
        Self { root, layout }
    }

    fn publish(&self, bytes: &[u8]) {
        let stage = Generations::new(&self.layout).stage().unwrap();
        stage.files().write("nested/artifact", bytes).unwrap();
        stage.commit().unwrap();
    }

    fn crash(&self, point: &str) {
        let output = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "generation::stage::tests::crash_child",
                "--nocapture",
            ])
            .env("OMEGA_CRASH_ROOT", &self.root)
            .env("OMEGA_CRASH_POINT", point)
            .output()
            .unwrap();
        assert_eq!(
            output.status.code(),
            Some(86),
            "checkpoint {point}: {}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn exit_at(point: &str) {
        if std::env::var("OMEGA_CRASH_POINT").unwrap() == point {
            // Process exit bypasses Rust destructors, including stage cleanup
            // and transaction guards. The kernel must release the locks.
            std::process::exit(86);
        }
    }

    fn assert_current(&self, expected: Option<&[u8]>) {
        let current = Generations::new(&self.layout).pin_current().unwrap();
        assert_eq!(
            current.map(|generation| {
                std::fs::read(generation.layout().state.join("nested/artifact")).unwrap()
            }),
            expected.map(<[u8]>::to_vec)
        );
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[test]
fn crash_child() {
    let Some(root) = std::env::var_os("OMEGA_CRASH_ROOT") else {
        return;
    };
    let fixture = Fixture::at(root.into());
    let stage = Generations::new(&fixture.layout).stage().unwrap();
    stage
        .files()
        .write("nested/artifact", b"candidate")
        .unwrap();
    let mut stage = stage;
    stage.checkpoint = Some(Fixture::exit_at);
    stage.commit().unwrap();
    panic!("the child did not reach its crash checkpoint");
}

#[test]
fn first_publication_survives_process_exit_at_each_commit_boundary() {
    for point in ["prepared", "published", "activated"] {
        let fixture = Fixture::new();
        fixture.crash(point);
        let expected = (point == "activated").then_some(b"candidate".as_slice());
        fixture.assert_current(expected);
        let store = Generations::new(&fixture.layout);
        assert!(store.recovery_ids().unwrap().is_empty());
        store.clean().unwrap();
        fixture.assert_current(expected);
        fixture.publish(b"next");
        fixture.assert_current(Some(b"next"));
    }
}

#[test]
fn interrupted_publication_preserves_acceptance_and_allows_rollback() {
    for point in ["prepared", "published", "activated"] {
        let fixture = Fixture::new();
        let store = Generations::new(&fixture.layout);
        fixture.publish(b"previous");
        let previous = store.pin_current().unwrap().unwrap();
        previous.accept().unwrap();
        let previous = previous.id().clone();
        fixture.publish(b"accepted");
        let accepted = store.pin_current().unwrap().unwrap();
        accepted.accept().unwrap();
        let accepted = accepted.id().clone();

        fixture.crash(point);
        let expected = if point == "activated" {
            b"candidate".as_slice()
        } else {
            b"accepted".as_slice()
        };
        fixture.assert_current(Some(expected));
        assert_eq!(
            store.recovery_ids().unwrap(),
            vec![accepted.clone(), previous]
        );
        store.clean().unwrap();
        fixture.assert_current(Some(expected));
        store.rollback(Some(&accepted)).unwrap().commit().unwrap();
        fixture.assert_current(Some(b"accepted"));
        store.clean().unwrap();
        fixture.publish(b"next");
        store.pin_current().unwrap().unwrap().accept().unwrap();
        assert_eq!(store.recovery_ids().unwrap()[1], accepted);
    }
}

#[test]
fn publication_refuses_a_nonempty_reserved_destination() {
    let fixture = Fixture::new();
    fixture.publish(b"accepted");
    let stage = Generations::new(&fixture.layout).stage().unwrap();
    stage
        .files()
        .write("nested/artifact", b"candidate")
        .unwrap();
    let occupied = stage.target.join("occupied");
    AtomicFile::at(&occupied).write(b"do not replace").unwrap();
    assert!(stage.commit().is_err());
    assert_eq!(std::fs::read(occupied).unwrap(), b"do not replace");
    fixture.assert_current(Some(b"accepted"));
}

#[test]
fn missing_directory_cannot_be_reported_as_durable() {
    let fixture = Fixture::new();
    assert_eq!(
        Directory::sync(&fixture.root).unwrap_err().kind(),
        std::io::ErrorKind::NotFound
    );
}
