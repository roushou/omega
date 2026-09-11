use super::StageDir;
use crate::TempPath;
use std::{io, path::PathBuf, process::Command};

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let root = TempPath::sibling(&std::env::temp_dir().join("omega-stage"), "test");
        std::fs::create_dir(&root).unwrap();
        Self(root)
    }

    fn stage(&self) -> StageDir {
        let stage = StageDir::new(&self.0.join("installed")).unwrap();
        stage.write("asset", b"new").unwrap();
        stage
    }

    fn old(&self) {
        let stage = StageDir::new(&self.0.join("installed")).unwrap();
        stage.write("obsolete", b"old").unwrap();
        stage.commit().unwrap();
    }

    fn assert_new(&self) {
        assert_eq!(
            std::fs::read(self.0.join("installed/asset")).unwrap(),
            b"new"
        );
        assert!(!self.0.join("installed/obsolete").exists());
    }

    fn exit_at(point: &str) -> io::Result<()> {
        if std::env::var("OMEGA_STAGE_POINT").unwrap() == point {
            std::process::exit(86);
        }
        Ok(())
    }

    fn fail_after_publish(point: &str) -> io::Result<()> {
        if point == "published" {
            return Err(io::Error::from_raw_os_error(libc::EIO));
        }
        Ok(())
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn crash_child() {
    let Some(root) = std::env::var_os("OMEGA_STAGE_ROOT") else {
        return;
    };
    let fixture = Fixture(root.into());
    let mut stage = fixture.stage();
    stage.checkpoint = Some(Fixture::exit_at);
    stage.commit().unwrap();
    panic!("checkpoint was not reached");
}

#[test]
fn publication_survives_exit_without_destructors() {
    for replace in [false, true] {
        for point in ["prepared", "published", "durable"] {
            let fixture = Fixture::new();
            if replace {
                fixture.old();
            }
            let output = Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "fs::stage::tests::crash_child"])
                .env("OMEGA_STAGE_ROOT", &fixture.0)
                .env("OMEGA_STAGE_POINT", point)
                .output()
                .unwrap();
            assert_eq!(output.status.code(), Some(86), "{output:?}");
            if point == "prepared" {
                if replace {
                    assert_eq!(
                        std::fs::read(fixture.0.join("installed/obsolete")).unwrap(),
                        b"old"
                    );
                } else {
                    assert!(!fixture.0.join("installed").exists());
                }
            } else {
                fixture.assert_new();
            }
            fixture.stage().commit().unwrap();
            fixture.assert_new();
        }
    }
}

#[test]
fn failed_publication_flush_preserves_displaced_entry_and_allows_retry() {
    let fixture = Fixture::new();
    fixture.old();
    let mut stage = fixture.stage();
    let displaced = stage.path().to_owned();
    stage.checkpoint = Some(Fixture::fail_after_publish);
    assert_eq!(stage.commit().unwrap_err().raw_os_error(), Some(libc::EIO));
    fixture.assert_new();
    assert_eq!(std::fs::read(displaced.join("obsolete")).unwrap(), b"old");
    fixture.stage().commit().unwrap();
    fixture.assert_new();
}

#[test]
fn replacing_links_never_removes_their_targets() {
    for dangling in [false, true] {
        let fixture = Fixture::new();
        let target = fixture.0.join("checkout");
        if !dangling {
            std::fs::create_dir(&target).unwrap();
            std::fs::write(target.join("untouched"), b"source").unwrap();
        }
        std::os::unix::fs::symlink(&target, fixture.0.join("installed")).unwrap();
        fixture.stage().commit().unwrap();
        fixture.assert_new();
        assert!(!fixture.0.join("installed").is_symlink());
        if !dangling {
            assert_eq!(std::fs::read(target.join("untouched")).unwrap(), b"source");
        }
        assert_eq!(
            std::fs::read_dir(&fixture.0).unwrap().count(),
            if dangling { 1 } else { 2 }
        );
    }
}

#[test]
fn failed_exchange_preserves_current_installation() {
    let fixture = Fixture::new();
    fixture.old();
    let stage = fixture.stage();
    std::fs::remove_dir_all(stage.path()).unwrap();
    assert!(stage.commit().is_err());
    assert_eq!(
        std::fs::read(fixture.0.join("installed/obsolete")).unwrap(),
        b"old"
    );
}
