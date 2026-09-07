//! The state document: authored, emitted, read back.

use std::path::PathBuf;

use omega_document::{Bars, Document, DocumentFile, Modules, Settings, Units};

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("omega-doc-{tag}-{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn document() -> omega_document::StateDocument {
    Document::new()
        .revision(7)
        .bar(Bars::top(
            "main",
            vec![
                Modules::clock("clock", "%H:%M"),
                Modules::plain_widget("battery", "battery-widget"),
            ],
        ))
        .setting(Settings::night_light("night", 3500))
        .unit(Units::disabled("noisy-unit"))
        .env("EDITOR", "hx")
        .into_inner()
}

#[test]
fn a_document_round_trips_through_its_canonical_form() {
    let encoded = DocumentFile::encode(&document()).unwrap();

    assert_eq!(DocumentFile::parse(&encoded).unwrap(), document());
    assert!(
        encoded.ends_with('\n'),
        "a committed file ends in a newline"
    );
    // Canonical protobuf JSON: enums by name, so a diff reads.
    assert!(encoded.contains("EDGE_TOP"), "{encoded}");
}

#[test]
fn a_document_survives_a_write_and_a_read() {
    let tmp = TempDir::new("write");
    let file = DocumentFile::at(tmp.0.join("document.json"));

    file.write(&document()).unwrap();

    assert_eq!(file.read().unwrap(), document());
    assert!(file.exists());
}

#[test]
fn a_config_without_a_document_is_a_valid_config() {
    let tmp = TempDir::new("absent");
    let file = DocumentFile::at(tmp.0.join("document.json"));

    // No `system/` crate: every built unit runs and nothing is claimed.
    assert!(!file.exists());
    assert_eq!(
        file.read_or_default().unwrap(),
        omega_document::StateDocument::default()
    );
    assert!(file.read().unwrap_err().is_not_found());
}

#[test]
fn a_corrupt_document_is_not_mistaken_for_an_absent_one() {
    let tmp = TempDir::new("corrupt");
    let path = tmp.0.join("document.json");
    std::fs::write(&path, "{not json").unwrap();
    let file = DocumentFile::at(&path);

    let err = file.read().unwrap_err();
    assert!(!err.is_not_found());
    assert!(err.to_string().contains("cannot parse"), "{err}");
    assert!(file.read_or_default().is_err());
}
