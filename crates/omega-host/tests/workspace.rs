//! Plugin discovery: the workspace manifest is the only registry.

use std::path::PathBuf;

use omega_host::Layout;
use omega_host::cargo::{CargoSlot, Manifest};
use omega_host::workspace::Plugins;
use omega_proto::PluginName;

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
    let source = format!(
        "[workspace]\nresolver = \"3\"\nmembers = {:?}\nexclude = {:?}\n",
        members, exclude
    );
    let manifest = source.parse::<Manifest>().unwrap();
    layout
        .file::<Manifest>(CargoSlot::Workspace)
        .write(&manifest)
        .unwrap();
}

#[test]
fn only_plugins_are_runnable_members() {
    let tmp = TempDir::new("discover");
    let layout = tmp.layout();
    for dir in ["plugins/battery", "plugins/clock", "crates/shared"] {
        std::fs::create_dir_all(layout.config.join(dir)).unwrap();
    }
    workspace(&layout, &["plugins/*", "crates/*"], &[]);

    let plugins = Plugins::discover(&layout).unwrap();
    assert_eq!(
        plugins.iter().collect::<Vec<_>>(),
        vec![
            &"battery".parse::<PluginName>().unwrap(),
            &"clock".parse::<PluginName>().unwrap(),
        ]
    );
}

#[test]
fn excluded_members_are_not_plugins() {
    let tmp = TempDir::new("exclude");
    let layout = tmp.layout();
    for dir in ["plugins/battery", "plugins/scratch"] {
        std::fs::create_dir_all(layout.config.join(dir)).unwrap();
    }
    workspace(&layout, &["plugins/*"], &["plugins/scratch"]);

    let plugins = Plugins::discover(&layout).unwrap();
    assert_eq!(plugins.len(), 1);
    assert_eq!(plugins.iter().next().unwrap().as_str(), "battery");
}

#[test]
fn an_invalid_member_name_fails_the_discovery() {
    let tmp = TempDir::new("invalid");
    let layout = tmp.layout();
    std::fs::create_dir_all(layout.config.join("plugins/Battery")).unwrap();
    workspace(&layout, &["plugins/*"], &[]);

    let err = Plugins::discover(&layout).unwrap_err();
    assert!(err.to_string().contains("lowercase"), "{err}");
}

#[test]
fn a_missing_workspace_manifest_is_reported_as_such() {
    let tmp = TempDir::new("no-manifest");
    let err = Plugins::discover(&tmp.layout()).unwrap_err();
    assert!(
        err.to_string().starts_with("cannot read cargo manifest"),
        "{err}"
    );
}

#[test]
fn misplaced_members_fail_loudly() {
    let tmp = TempDir::new("roles");
    let layout = tmp.layout();
    for member in ["plugins/nested/deep", "shared", "../external"] {
        workspace(&layout, &[member], &[]);
        assert!(
            Plugins::discover(&layout)
                .unwrap_err()
                .to_string()
                .contains("outside")
        );
    }
}
