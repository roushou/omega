//! `units.toml`: what a build produced, written by one program and read by
//! another.
//!
//! These exercise the TOML machinery too — reading, writing, editing, the
//! long-lived document — but through the document that actually crosses the
//! boundary, so the assertions are about a file both programs depend on
//! rather than about a fixture.

use std::path::{Path, PathBuf};

use omega_core::{Layout, Toml, UnitName};
use omega_manifest::{BuiltUnit, StateConfig};

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("omega-state-{tag}-{nanos}"));
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

fn unit(name: &str) -> UnitName {
    UnitName::parse(name).unwrap()
}

#[test]
fn the_schema_puts_it_where_the_layout_says() {
    let tmp = TempDir::new("locate");
    let layout = tmp.layout();

    // The layout places the file and the document declares what fills it.
    // One name, in one place, or a staged copy and the final one disagree.
    assert_eq!(
        layout.file::<StateConfig>(()).path(),
        layout.state_units_toml()
    );
    assert_eq!(StateConfig::FILE_NAME, Layout::UNITS_TOML);
}

#[test]
fn write_then_read_round_trips() {
    let tmp = TempDir::new("round-trip");
    let layout = tmp.layout();
    let file = layout.file::<StateConfig>(());

    let config = StateConfig::new(&layout, [unit("a-unit"), unit("b-unit")]);
    file.write(&config).unwrap();

    assert_eq!(file.read().unwrap(), config);
    assert_eq!(
        config.units[0].program,
        Path::new("units/a-unit/a-unit").to_path_buf()
    );
}

#[test]
fn missing_file_is_distinguishable_from_a_broken_one() {
    let tmp = TempDir::new("missing");
    let layout = tmp.layout();
    let file = layout.file::<StateConfig>(());

    let err = file.read().unwrap_err();
    assert!(err.is_not_found());
    assert_eq!(err.path(), Some(layout.state_units_toml().as_path()));
    assert_eq!(file.read_or_default().unwrap(), StateConfig::default());

    std::fs::create_dir_all(&layout.state).unwrap();
    std::fs::write(layout.state_units_toml(), "units = 3\n").unwrap();
    let err = file.read().unwrap_err();
    assert!(!err.is_not_found());
    assert!(err.to_string().starts_with("cannot parse state config"));
    assert!(file.read_or_default().is_err());
}

#[test]
fn create_new_never_overwrites() {
    let tmp = TempDir::new("create-new");
    let layout = tmp.layout();
    let file = layout.file::<StateConfig>(());

    let first = StateConfig::new(&layout, [unit("a-unit")]);
    assert!(file.create_new(&first).unwrap());
    assert!(!file.create_new(&StateConfig::default()).unwrap());
    assert_eq!(file.read().unwrap(), first);
}

#[test]
fn edit_reads_mutates_and_writes_back() {
    let tmp = TempDir::new("edit");
    let layout = tmp.layout();
    let file = layout.file::<StateConfig>(());
    file.write(&StateConfig::new(&layout, [unit("a-unit")]))
        .unwrap();

    let added = file
        .edit(|config| {
            config.units.push(BuiltUnit::new(&layout, unit("b-unit")));
            config.units.len()
        })
        .unwrap();

    assert_eq!(added, 2);
    assert_eq!(file.read().unwrap().units.len(), 2);
}

#[test]
fn a_doc_is_changed_across_steps_and_saved_once() {
    let tmp = TempDir::new("doc-steps");
    let layout = tmp.layout();
    let file = layout.file::<StateConfig>(());
    file.write(&StateConfig::default()).unwrap();

    // The value outlives the call that loaded it: several decisions are made
    // against one read, and the file is written once at the end.
    let mut doc = file.open().unwrap();
    for name in ["a-unit", "b-unit", "c-unit"] {
        doc.value_mut()
            .units
            .push(BuiltUnit::new(&layout, unit(name)));
    }

    // Nothing has touched the file yet.
    assert!(file.read().unwrap().units.is_empty());

    doc.save().unwrap();
    assert_eq!(file.read().unwrap().units.len(), 3);
    assert_eq!(doc.value().units.len(), 3);
}

#[test]
fn doc_saves_back_to_its_own_file() {
    let tmp = TempDir::new("doc");
    let layout = tmp.layout();
    let file = layout.file::<StateConfig>(());
    file.write(&StateConfig::default()).unwrap();

    let mut doc = file.open().unwrap();
    doc.value_mut()
        .units
        .push(BuiltUnit::new(&layout, unit("a-unit")));
    doc.save().unwrap();

    assert_eq!(doc.path(), file.path());
    assert_eq!(file.read().unwrap().units.len(), 1);
}

#[test]
fn a_schema_declares_which_tables_hold_inline_entries() {
    // `units.toml` declares none, so its entries stay as sections.
    let tmp = TempDir::new("inline");
    let layout = tmp.layout();
    let encoded = Toml::encode(&StateConfig::new(&layout, [unit("a-unit")])).unwrap();

    assert_eq!(
        encoded,
        r#"[[units]]
name = "a-unit"
program = "units/a-unit/a-unit"
manifest = "units/a-unit/unit.toml"
"#
    );
}
