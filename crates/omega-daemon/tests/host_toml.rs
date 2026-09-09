//! The TOML stack: schemas, typed files, and the Cargo model.

use std::path::{Path, PathBuf};

use omega_daemon::host::cargo::{
    CargoManifest, CargoSlot, Dependencies, Dependency, DependencySpec, Package, Workspace,
};
use omega_host::Layout;
use omega_host::{Table, Toml, TomlFile};
use omega_proto::UnitName;

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

fn unit(name: &str) -> UnitName {
    UnitName::parse(name).unwrap()
}

#[test]
fn schema_locates_every_instance() {
    let tmp = TempDir::new("locate");
    let layout = tmp.layout();
    let name = unit("battery-widget");

    assert_eq!(
        layout.file::<CargoManifest>(CargoSlot::Workspace).path(),
        layout.workspace_manifest()
    );
    assert_eq!(
        layout.file::<CargoManifest>(CargoSlot::Unit(&name)).path(),
        layout.unit_crate_manifest(&name)
    );
}

#[test]
fn unmodelled_cargo_keys_survive_a_rewrite() {
    let tmp = TempDir::new("rest");
    let file = TomlFile::<CargoManifest>::at(tmp.path().join("Cargo.toml"));
    std::fs::write(
        file.path(),
        r#"
[workspace]
members = ["units/*"]

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
            .workspace
            .as_mut()
            .unwrap()
            .members
            .push("lib/shared".into())
    })
    .unwrap();

    let rewritten = std::fs::read_to_string(file.path()).unwrap();
    assert!(rewritten.contains("lib/shared"), "{rewritten}");
    assert!(rewritten.contains("patch.crates-io"), "{rewritten}");
    assert!(rewritten.contains("vendor/serde"), "{rewritten}");
    assert!(rewritten.contains("license"), "{rewritten}");
    assert!(rewritten.contains("optional = true"), "{rewritten}");

    let manifest = file.read().unwrap();
    let deps = &manifest.workspace.as_ref().unwrap().dependencies;
    assert_eq!(deps.get("serde").unwrap().version(), Some("1"));
    assert_eq!(deps.get("tracing").unwrap().version(), Some("0.1"));
}

#[test]
fn cargo_manifest_encodes_the_shape_cargo_expects() {
    let manifest = CargoManifest {
        package: Some(Package::new("battery-widget", "0.1.0", "2024")),
        dependencies: Dependencies::from_iter([
            ("omega", Dependency::inherited()),
            ("tokio", Dependency::registry("1", &["macros"])),
            ("anyhow", Dependency::registry("1", &[])),
            ("local", Dependency::local("/src/local", &[])),
        ]),
        ..Default::default()
    };

    let encoded = Toml::encode(&manifest).unwrap();
    assert_eq!(Toml::decode::<CargoManifest>(&encoded).unwrap(), manifest);
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
fn a_member_crate_inherits_exactly_the_declared_dependencies() {
    const SPECS: &[DependencySpec] = &[
        DependencySpec::registry("tokio", "1").with_features(&["macros"]),
        DependencySpec::omega("omega"),
    ];

    let inherited = Dependencies::from_iter(SPECS.iter().map(DependencySpec::inherited));

    assert_eq!(
        inherited.names().collect::<Vec<_>>(),
        vec!["omega", "tokio"]
    );
    assert!(inherited.iter().all(|(_, d)| d.is_inherited()));
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
    for dir in ["units/alpha", "units/beta", "units/.hidden", "lib/shared"] {
        std::fs::create_dir_all(tmp.path().join(dir)).unwrap();
    }

    let workspace = Workspace {
        members: vec!["units/*".into(), "lib/shared".into()],
        exclude: vec!["units/beta".into()],
        ..Default::default()
    };

    let dirs = workspace.member_dirs(tmp.path()).unwrap();
    assert_eq!(
        dirs,
        vec![
            tmp.path().join("lib/shared"),
            tmp.path().join("units/.hidden"),
            tmp.path().join("units/alpha"),
        ]
    );
}
