//! Built-plugin index serialization and typed TOML edit tests.

use std::path::{Path, PathBuf};

use omega_host::Layout;
use omega_host::Toml;
use omega_host::{BuiltPlugin, StateConfig};
use omega_proto::PluginName;

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

fn plugin(name: &str) -> PluginName {
    PluginName::try_from(name).unwrap()
}

#[test]
fn the_schema_puts_it_where_the_layout_says() {
    let tmp = TempDir::new("locate");
    let layout = tmp.layout();

    // The layout places the file and the document declares what fills it.
    // One name, in one place, or a staged copy and the final one disagree.
    assert_eq!(
        layout.file::<StateConfig>(()).path(),
        layout.state_plugins_toml()
    );
    assert_eq!(StateConfig::FILE_NAME, Layout::PLUGINS_TOML);
}

#[test]
fn write_then_read_round_trips() {
    let tmp = TempDir::new("round-trip");
    let layout = tmp.layout();
    let file = layout.file::<StateConfig>(());

    let config = StateConfig::new(&layout, [plugin("a-plugin"), plugin("b-plugin")]);
    file.write(&config).unwrap();

    assert_eq!(file.read().unwrap(), config);
    assert_eq!(
        config.plugins[0].program,
        Path::new("plugins/a-plugin/a-plugin").to_path_buf()
    );
}

#[test]
fn missing_file_is_distinguishable_from_a_broken_one() {
    let tmp = TempDir::new("missing");
    let layout = tmp.layout();
    let file = layout.file::<StateConfig>(());

    let err = file.read().unwrap_err();
    assert!(err.is_not_found());
    assert_eq!(err.path(), Some(layout.state_plugins_toml().as_path()));
    assert_eq!(file.read_or_default().unwrap(), StateConfig::default());

    std::fs::create_dir_all(&layout.state).unwrap();
    std::fs::write(layout.state_plugins_toml(), "plugins = 3\n").unwrap();
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

    let first = StateConfig::new(&layout, [plugin("a-plugin")]);
    assert!(file.create_new(&first).unwrap());
    assert!(!file.create_new(&StateConfig::default()).unwrap());
    assert_eq!(file.read().unwrap(), first);
}

#[test]
fn edit_reads_mutates_and_writes_back() {
    let tmp = TempDir::new("edit");
    let layout = tmp.layout();
    let file = layout.file::<StateConfig>(());
    file.write(&StateConfig::new(&layout, [plugin("a-plugin")]))
        .unwrap();

    let added = file
        .edit(|config| {
            config
                .plugins
                .push(BuiltPlugin::new(&layout, plugin("b-plugin")));
            config.plugins.len()
        })
        .unwrap();

    assert_eq!(added, 2);
    assert_eq!(file.read().unwrap().plugins.len(), 2);
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
    for name in ["a-plugin", "b-plugin", "c-plugin"] {
        doc.value_mut()
            .plugins
            .push(BuiltPlugin::new(&layout, plugin(name)));
    }

    assert_eq!(doc.path(), file.path());

    // Nothing has touched the file yet.
    assert!(file.read().unwrap().plugins.is_empty());

    doc.save().unwrap();
    assert_eq!(file.read().unwrap().plugins.len(), 3);
    assert_eq!(doc.value().plugins.len(), 3);
}

#[test]
fn a_schema_declares_which_tables_hold_inline_entries() {
    // `plugins.toml` declares none, so its entries stay as sections.
    let tmp = TempDir::new("inline");
    let layout = tmp.layout();
    let encoded = Toml::encode(&StateConfig::new(&layout, [plugin("a-plugin")])).unwrap();

    assert_eq!(
        encoded,
        r#"[[plugins]]
name = "a-plugin"
program = "plugins/a-plugin/a-plugin"
manifest = "plugins/a-plugin/plugin.pb"
"#
    );
}
