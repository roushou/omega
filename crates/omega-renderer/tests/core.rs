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
                found.insert(
                    path.strip_prefix(root)
                        .unwrap()
                        .to_string_lossy()
                        .replace('\\', "/"),
                );
            }
        }
    }
}

#[test]
fn every_core_asset_is_embedded_and_has_no_omarchy_imports() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("shell/core");
    let mut found = BTreeSet::new();
    Sources::files(&root, root.parent().unwrap(), &mut found);
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
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("shell/preview");
    let mut found = BTreeSet::new();
    Sources::files(&root, &root, &mut found);
    let mut embedded = BTreeSet::new();
    for asset in omega_renderer::Preview::FILES {
        let source = asset.name.strip_prefix("preview/").unwrap_or(asset.name);
        assert!(
            embedded.insert(source.to_string()),
            "duplicate preview asset: {source}"
        );
        assert_eq!(
            std::fs::read_to_string(root.join(source)).unwrap(),
            asset.contents,
            "preview asset {source}"
        );
    }
    assert_eq!(found, embedded);
}
