//! The TOML stack: schemas, typed files, and the Cargo model.

use std::path::{Path, PathBuf};

use omega_host::Layout;
use omega_host::cargo::{CargoSlot, Dependencies, Dependency, Inherited, Manifest};
use omega_host::{Table, Toml, TomlFile};
use omega_proto::PluginName;

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("omega-core-{tag}-{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    fn path(&self) -> &Path {
        &self.0
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
fn schema_locates_every_instance() {
    let tmp = TempDir::new("locate");
    let layout = tmp.layout();
    let name = plugin("battery-widget");

    assert_eq!(
        layout.file::<Manifest>(CargoSlot::Workspace).path(),
        layout.workspace_manifest()
    );
    assert_eq!(
        layout.file::<Manifest>(CargoSlot::Plugin(&name)).path(),
        layout.plugin_crate_manifest(&name)
    );
}

#[test]
fn unmodelled_cargo_keys_survive_a_rewrite() {
    let tmp = TempDir::new("rest");
    let file = TomlFile::<Manifest>::at(tmp.path().join("Cargo.toml"));
    std::fs::write(
        file.path(),
        r#"
[workspace]
members = ["plugins/*"]

[workspace.package]
license = "MIT"

[workspace.dependencies]
serde = "1"
tracing = { version = "0.1", optional = true }

[patch.crates-io]
serde = { path = "vendor/serde" }
"#,
    )
    .unwrap();

    file.edit(|manifest| {
        manifest
            .ensure_member("crates/shared", "crates/shared")
            .unwrap()
    })
    .unwrap();

    let rewritten = std::fs::read_to_string(file.path()).unwrap();
    assert!(rewritten.contains("crates/shared"), "{rewritten}");
    assert!(rewritten.contains("patch.crates-io"), "{rewritten}");
    assert!(rewritten.contains("vendor/serde"), "{rewritten}");
    assert!(rewritten.contains("license"), "{rewritten}");
    assert!(rewritten.contains("optional = true"), "{rewritten}");

    let manifest = file.read().unwrap();
    let deps = manifest
        .workspace()
        .unwrap()
        .unwrap()
        .dependencies()
        .unwrap();
    assert_eq!(deps.get("serde").unwrap().version(), Some("1"));
    assert_eq!(deps.get("tracing").unwrap().version(), Some("0.1"));
}

#[test]
fn cargo_manifest_encodes_the_shape_cargo_expects() {
    let manifest = Manifest::new_package(
        "battery-widget",
        "0.1.0",
        Inherited::Value("2024"),
        &Dependencies::from_iter([
            ("omega", Dependency::inherited()),
            ("tokio", Dependency::registry("1", &["macros"])),
            ("anyhow", Dependency::registry("1", &[])),
            ("local", Dependency::local("/src/local", &[])),
        ]),
    )
    .unwrap();

    let encoded = Toml::encode(&manifest).unwrap();
    assert_eq!(
        Toml::decode::<Manifest>(&encoded).unwrap().to_string(),
        manifest.to_string()
    );
    assert!(!encoded.contains("[rest]"), "{encoded}");

    // Dependencies are inline entries under one `[dependencies]` section,
    // the shape Cargo and every hand-written manifest use.
    assert_eq!(
        encoded,
        r#"[package]
name = "battery-widget"
version = "0.1.0"
edition = "2024"

[dependencies]
anyhow = "1"
local = { path = "/src/local" }
omega = { workspace = true }
tokio = { version = "1", features = ["macros"] }
"#
    );
}

#[test]
fn tables_are_alphabetical_regardless_of_insertion_order() {
    let mut table = Table::from_iter([("zeta", 1), ("alpha", 2)]);
    table.insert("mid", 3);
    assert_eq!(
        table.names().collect::<Vec<_>>(),
        vec!["alpha", "mid", "zeta"]
    );
    assert_eq!(table.get("mid"), Some(&3));
    assert_eq!(table.remove("mid"), Some(3));
    assert!(!table.contains("mid"));
}

#[test]
fn workspace_members_expand_globs_and_honour_exclude() {
    let tmp = TempDir::new("members");
    for dir in [
        "plugins/alpha",
        "plugins/beta",
        "plugins/.hidden",
        "crates/shared",
    ] {
        std::fs::create_dir_all(tmp.path().join(dir)).unwrap();
    }

    let manifest = (r#"[workspace]
members = ["plugins/*", "crates/shared"]
exclude = ["plugins/beta"]
"#)
    .parse::<Manifest>()
    .unwrap();
    let dirs = manifest
        .workspace()
        .unwrap()
        .unwrap()
        .member_dirs(tmp.path())
        .unwrap();
    assert_eq!(
        dirs,
        vec![
            tmp.path().join("crates/shared"),
            tmp.path().join("plugins/.hidden"),
            tmp.path().join("plugins/alpha"),
        ]
    );
}

#[test]
fn typed_file_roundtrips_preserve_source_bytes_and_report_parse_paths() {
    use omega_host::cargo::Config;

    let tmp = TempDir::new("source-codec");
    let layout = tmp.layout();
    let manifest = layout.file::<Manifest>(CargoSlot::Workspace);
    let config = layout.file::<Config>(());
    let manifest_source = "# user manifest\n[package]\nname = 'desktop'\nversion.workspace = true # inheritance\n[workspace]\nmembers = []\n";
    let config_source = "# user settings\n[build]\njobs = 2 # keep\n";

    manifest
        .write(&manifest_source.parse::<Manifest>().unwrap())
        .unwrap();
    config
        .write(&config_source.parse::<Config>().unwrap())
        .unwrap();
    manifest.open().unwrap().save().unwrap();
    config.open().unwrap().save().unwrap();

    assert_eq!(
        std::fs::read_to_string(manifest.path()).unwrap(),
        manifest_source
    );
    assert_eq!(
        std::fs::read_to_string(config.path()).unwrap(),
        config_source
    );

    std::fs::write(manifest.path(), "[broken").unwrap();
    let error = manifest.read().unwrap_err();
    assert_eq!(error.path(), Some(manifest.path()));
    assert!(error.to_string().contains("cargo manifest"));
    assert!(error.to_string().contains("line 1"));
    assert!(!error.to_string().contains("cannot decode"));
}
