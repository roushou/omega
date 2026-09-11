//! Noticing a rebuild.

use std::path::PathBuf;

use omega_daemon::watch::StateStamp;
use omega_host::Layout;

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("omega-watch-{tag}-{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    fn layout(&self) -> Layout {
        Layout::at(
            self.0.join("config"),
            self.0.join("state"),
            self.0.join("cache"),
        )
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn an_absent_state_dir_is_a_stable_stamp() {
    let tmp = TempDir::new("absent");
    let layout = tmp.layout();

    // A daemon started before the first build must not reload every second.
    let stamp = StateStamp::of(&layout);
    assert!(!stamp.changed(&layout));
}

#[test]
fn only_publishing_a_generation_changes_the_stamp() {
    let tmp = TempDir::new("rebuild");
    let layout = tmp.layout();
    let stamp = StateStamp::of(&layout);
    let generation = omega_host::Generations::new(&layout).stage().unwrap();
    generation.files().write("document.json", b"{}").unwrap();
    assert!(!stamp.changed(&layout));
    generation.commit().unwrap();
    assert!(stamp.changed(&layout));
    let stamp = StateStamp::of(&layout);
    let abandoned = omega_host::Generations::new(&layout).stage().unwrap();
    abandoned
        .files()
        .write("document.json", b"changed")
        .unwrap();
    drop(abandoned);
    assert!(!stamp.changed(&layout));
}
