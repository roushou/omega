//! Noticing a rebuild.

use std::path::PathBuf;

use omega_daemon::host::StateConfig;
use omega_daemon::watch::StateStamp;
use omega_document::{Document, DocumentFile, Units};
use omega_host::Layout;
use omega_proto::UnitName;

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
fn writing_either_half_of_the_build_is_a_change() {
    let tmp = TempDir::new("rebuild");
    let layout = tmp.layout();
    std::fs::create_dir_all(&layout.state).unwrap();

    let units = layout.file::<StateConfig>(());
    units
        .write(&StateConfig::new(
            &layout,
            [UnitName::parse("a-unit").unwrap()],
        ))
        .unwrap();
    let document = DocumentFile::of(&layout);
    document.write(&Document::new().into_inner()).unwrap();

    let stamp = StateStamp::of(&layout);
    assert!(!stamp.changed(&layout));

    // What the machine is for changed...
    document
        .write(&Document::new().unit(Units::disabled("a-unit")).into_inner())
        .unwrap();
    assert!(stamp.changed(&layout));

    // ...and so did what was built.
    let stamp = StateStamp::of(&layout);
    units.write(&StateConfig::default()).unwrap();
    assert!(stamp.changed(&layout));
}
