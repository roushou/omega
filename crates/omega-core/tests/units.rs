//! Unit discovery: the workspace manifest is the only registry.

use std::path::PathBuf;

use omega_core::toml::cargo::{CargoManifest, CargoSlot, Workspace};
use omega_core::{Layout, UnitName, Units};

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("omegas-{tag}-{nanos}"));
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

fn workspace(layout: &Layout, members: &[&str], exclude: &[&str]) {
    let manifest = CargoManifest {
        workspace: Some(Workspace {
            resolver: Some("3".into()),
            members: members.iter().map(|m| (*m).to_string()).collect(),
            exclude: exclude.iter().map(|e| (*e).to_string()).collect(),
            ..Default::default()
        }),
        ..Default::default()
    };
    layout
        .file::<CargoManifest>(CargoSlot::Workspace)
        .write(&manifest)
        .unwrap();
}

#[test]
fn units_are_the_members_directly_under_units() {
    let tmp = TempDir::new("discover");
    let layout = tmp.layout();
    for dir in [
        "units/battery",
        "units/clock",
        "units/nested/deep",
        "lib/shared",
    ] {
        std::fs::create_dir_all(layout.config.join(dir)).unwrap();
    }
    workspace(&layout, &["units/*", "units/nested/*", "lib/*"], &[]);

    let units = Units::discover(&layout).unwrap();
    assert_eq!(
        units.iter().collect::<Vec<_>>(),
        vec![
            &UnitName::parse("battery").unwrap(),
            &UnitName::parse("clock").unwrap(),
            &UnitName::parse("nested").unwrap(),
        ]
    );
}

#[test]
fn excluded_members_are_not_units() {
    let tmp = TempDir::new("exclude");
    let layout = tmp.layout();
    for dir in ["units/battery", "units/scratch"] {
        std::fs::create_dir_all(layout.config.join(dir)).unwrap();
    }
    workspace(&layout, &["units/*"], &["units/scratch"]);

    let units = Units::discover(&layout).unwrap();
    assert_eq!(units.len(), 1);
    assert_eq!(units.iter().next().unwrap().as_str(), "battery");
}

#[test]
fn an_invalid_member_name_fails_the_discovery() {
    let tmp = TempDir::new("invalid");
    let layout = tmp.layout();
    std::fs::create_dir_all(layout.config.join("units/Battery")).unwrap();
    workspace(&layout, &["units/*"], &[]);

    let err = Units::discover(&layout).unwrap_err();
    assert!(err.to_string().contains("lowercase"), "{err}");
}

#[test]
fn a_missing_workspace_manifest_is_reported_as_such() {
    let tmp = TempDir::new("no-manifest");
    let err = Units::discover(&tmp.layout()).unwrap_err();
    assert!(
        err.to_string().starts_with("cannot read cargo manifest"),
        "{err}"
    );
}
