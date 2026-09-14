use omega_renderer::Core;
use std::collections::BTreeSet;
use std::path::Path;

struct Sources;
impl Sources {
    fn files(directory: &Path, root: &Path, found: &mut BTreeSet<String>) {
        for entry in std::fs::read_dir(directory).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                Self::files(&path, root, found);
            } else {
                found.insert(format!(
                    "core/{}",
                    path.strip_prefix(root).unwrap().display()
                ));
            }
        }
    }
}

#[test]
fn every_core_asset_is_embedded_and_has_no_omarchy_imports() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("shell/core");
    let mut found = BTreeSet::new();
    Sources::files(&root, &root, &mut found);
    assert_eq!(
        found,
        Core::FILES.iter().map(|a| a.name.to_owned()).collect()
    );
    for asset in Core::FILES {
        assert!(
            !asset
                .contents
                .lines()
                .any(|line| line.trim_start().starts_with("import qs.")),
            "{} imports a desktop host",
            asset.name
        );
    }
}

#[test]
fn preview_assets_are_embedded() {
    let names: BTreeSet<_> = omega_renderer::Preview::FILES
        .iter()
        .map(|a| a.name)
        .collect();
    assert_eq!(
        names,
        BTreeSet::from(["shell.qml", "preview/Preview.qml", "preview/Viewport.qml"])
    );
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("shell/preview");
    assert_eq!(std::fs::read_dir(root).unwrap().count(), names.len());
}
