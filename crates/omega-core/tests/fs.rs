//! Filesystem primitives: atomicity, durability, and what a fingerprint sees.

use std::path::{Path, PathBuf};
use std::time::Duration;

use omega_core::{AtomicFile, Changes, Recursion, StageDir};

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("omega-fs-{tag}-{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn entries(dir: &Path) -> Vec<String> {
    let mut names: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

#[test]
fn a_write_creates_parents_and_leaves_no_temp_behind() {
    let tmp = TempDir::new("atomic");
    let target = tmp.path().join("nested/deep/unit.toml");

    AtomicFile::at(&target)
        .write(b"name = \"battery\"\n")
        .unwrap();

    assert_eq!(
        std::fs::read_to_string(&target).unwrap(),
        "name = \"battery\"\n"
    );
    assert_eq!(entries(target.parent().unwrap()), vec!["unit.toml"]);
}

#[test]
fn a_rewrite_replaces_the_contents_entirely() {
    let tmp = TempDir::new("atomic-replace");
    let target = tmp.path().join("units.toml");
    let file = AtomicFile::at(&target);

    file.write(b"first, much longer contents").unwrap();
    file.write(b"second").unwrap();

    assert_eq!(std::fs::read_to_string(&target).unwrap(), "second");
    assert_eq!(entries(tmp.path()), vec!["units.toml"]);
}

#[test]
fn a_dropped_stage_never_touches_the_live_directory() {
    let tmp = TempDir::new("stage-drop");
    let live = tmp.path().join("state");
    std::fs::create_dir_all(&live).unwrap();
    std::fs::write(live.join("units.toml"), "old").unwrap();

    let stage = StageDir::new(&live).unwrap();
    stage.write("units.toml", b"new").unwrap();
    drop(stage); // a failed build

    assert_eq!(
        std::fs::read_to_string(live.join("units.toml")).unwrap(),
        "old"
    );
    assert_eq!(entries(tmp.path()), vec!["state"], "no stage left behind");
}

#[test]
fn a_committed_stage_replaces_the_live_directory_wholesale() {
    let tmp = TempDir::new("stage-commit");
    let live = tmp.path().join("state");
    std::fs::create_dir_all(live.join("units")).unwrap();
    std::fs::write(live.join("gone.toml"), "stale").unwrap();

    let stage = StageDir::new(&live).unwrap();
    stage.write("units.toml", b"new").unwrap();
    stage.commit().unwrap();

    assert_eq!(entries(&live), vec!["units.toml"], "the old tree is gone");
    assert_eq!(entries(tmp.path()), vec!["state"], "no backup left behind");
}

#[tokio::test]
async fn a_watch_reports_a_saved_file_once() {
    let tmp = TempDir::new("watch");
    let config = tmp.path().join("config/units/battery/src");
    std::fs::create_dir_all(&config).unwrap();

    let mut changes = Changes::with_settle(
        &[tmp.path().join("config").as_path()],
        Recursion::Recursive,
        Duration::from_millis(50),
    )
    .unwrap();

    // A save is many writes; it is one change.
    std::fs::write(config.join("main.rs"), "fn main() {}").unwrap();
    std::fs::write(config.join("main.rs"), "fn main() { todo!() }").unwrap();

    tokio::time::timeout(Duration::from_secs(2), changes.next())
        .await
        .expect("a saved file should be reported")
        .expect("the watch should still be live");

    // And nothing more until something else happens.
    assert!(
        tokio::time::timeout(Duration::from_millis(300), changes.next())
            .await
            .is_err(),
        "a settled watch should be quiet"
    );
}

#[tokio::test]
async fn reading_the_config_is_not_changing_it() {
    let tmp = TempDir::new("watch-read");
    let config = tmp.path().join("config");
    std::fs::create_dir_all(&config).unwrap();
    std::fs::write(config.join("Cargo.toml"), "[workspace]").unwrap();

    let mut changes = Changes::with_settle(
        &[config.as_path()],
        Recursion::Recursive,
        Duration::from_millis(50),
    )
    .unwrap();

    // A build reads every file in the config. If that reads as a change, the
    // watcher rebuilds because it built, forever.
    for _ in 0..5 {
        let _ = std::fs::read_to_string(config.join("Cargo.toml")).unwrap();
    }

    assert!(
        tokio::time::timeout(Duration::from_millis(400), changes.next())
            .await
            .is_err(),
        "opening a file is not editing it"
    );

    // Writing to it still is.
    std::fs::write(config.join("Cargo.toml"), "[workspace]\nmembers = []").unwrap();
    tokio::time::timeout(Duration::from_secs(2), changes.next())
        .await
        .expect("an edit should be reported")
        .unwrap();
}

#[tokio::test]
async fn a_build_directory_appearing_is_not_a_change() {
    let tmp = TempDir::new("watch-appearing");
    let config = tmp.path().join("config");
    std::fs::create_dir_all(&config).unwrap();

    let mut changes = Changes::with_settle(
        &[config.as_path()],
        Recursion::Recursive,
        Duration::from_millis(50),
    )
    .unwrap();

    // The first `cargo build` inside a config dir creates `target/` from
    // nothing; if that reads as a change, a watcher rebuilds because it
    // built.
    std::fs::create_dir_all(config.join("target/release")).unwrap();
    std::fs::write(config.join("target/release/battery"), "binary").unwrap();

    assert!(
        tokio::time::timeout(Duration::from_millis(400), changes.next())
            .await
            .is_err(),
        "creating build output is not a config change"
    );
}

#[tokio::test]
async fn a_watch_ignores_build_output() {
    let tmp = TempDir::new("watch-ignored");
    let config = tmp.path().join("config");
    std::fs::create_dir_all(config.join("target/release")).unwrap();
    std::fs::create_dir_all(config.join(".git")).unwrap();

    let mut changes = Changes::with_settle(
        &[config.as_path()],
        Recursion::Recursive,
        Duration::from_millis(50),
    )
    .unwrap();

    // A build inside the config dir must not look like a config change, or a
    // watcher would rebuild forever.
    std::fs::write(config.join("target/release/battery"), "binary").unwrap();
    std::fs::write(config.join(".git/HEAD"), "ref: refs/heads/main").unwrap();

    assert!(
        tokio::time::timeout(Duration::from_millis(400), changes.next())
            .await
            .is_err(),
        "build output is not a config change"
    );

    // A real edit still registers.
    std::fs::write(config.join("Cargo.toml"), "[workspace]").unwrap();
    tokio::time::timeout(Duration::from_secs(2), changes.next())
        .await
        .expect("an edit should be reported")
        .unwrap();
}
