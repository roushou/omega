//! Verify published crates include all compile-time assets and versioned dependencies.

use std::path::{Path, PathBuf};

/// The checkout these tests run inside: `crates/omega-cli` → `crates` → root.
fn checkout() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("omega-cli lives two directories under the checkout")
        .to_path_buf()
}

fn crate_roots() -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = std::fs::read_dir(checkout().join("crates"))
        .expect("the workspace has a crates directory")
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.join("Cargo.toml").is_file())
        .collect();
    roots.sort();
    roots
}

/// Every `.rs` file a crate compiles, which is where an `include_str!` can be.
fn sources(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let build = root.join("build.rs");
    if build.is_file() {
        found.push(build);
    }
    walk(&root.join("src"), &mut found);
    found.sort();
    found
}

fn walk(dir: &Path, found: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        match path.is_dir() {
            true => walk(&path, found),
            false if path.extension().is_some_and(|ext| ext == "rs") => found.push(path),
            false => {}
        }
    }
}

/// Count parent-directory traversal in include macros' leading literals.
fn climbs(source: &str) -> Vec<(String, usize)> {
    let mut found = Vec::new();
    for macro_name in ["include_str!", "include_bytes!"] {
        let mut rest = source;
        while let Some(at) = rest.find(macro_name) {
            rest = &rest[at + macro_name.len()..];
            let Some(end) = rest.find(')') else { break };
            let Some(first) = rest[..end].split('"').nth(1) else {
                continue;
            };
            let climb = first
                .split('/')
                .take_while(|part| matches!(*part, ".." | "." | ""))
                .filter(|part| *part == "..")
                .count();
            found.push((first.to_string(), climb));
        }
    }
    found
}

/// How many directories deep in its crate a file sits, which is how far it
/// may climb and still be inside.
fn depth(file: &Path, root: &Path) -> usize {
    file.parent()
        .and_then(|dir| dir.strip_prefix(root).ok())
        .map(|rel| rel.components().count())
        .unwrap_or(0)
}

/// Package manifest model supporting workspace-inherited metadata.
fn manifest(path: &Path) -> toml::Value {
    let source =
        std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    toml::from_str(&source).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// The dependency tables in one manifest: the workspace's, and the crate's.
fn dependency_tables(manifest: &toml::Value) -> Vec<&toml::Table> {
    ["workspace.dependencies", "dependencies"]
        .into_iter()
        .filter_map(|path| {
            path.split('.')
                .try_fold(manifest, |value, key| value.get(key))?
                .as_table()
        })
        .collect()
}

#[test]
fn nothing_a_crate_compiles_reaches_above_its_own_root() {
    let mut checked = 0;

    for root in crate_roots() {
        for file in sources(&root) {
            let source = std::fs::read_to_string(&file).unwrap();
            let allowed = depth(&file, &root);

            for (path, climb) in climbs(&source) {
                checked += 1;
                assert!(
                    climb <= allowed,
                    "{} includes {path}, which climbs {climb} out of a file {allowed} deep — \
                     past {} and out of the published crate",
                    file.display(),
                    root.display()
                );
            }
        }
    }

    // A guard that guards nothing passes just as quietly as one that works,
    // and this file has already been emptied once without anything noticing.
    assert!(checked > 0, "no include_str! was found to check");
}

#[test]
fn the_schema_a_published_proto_crate_compiles_is_inside_it() {
    // Require schemas to be packaged with their build script.
    let wire = checkout().join("crates/omega-proto");
    let build = std::fs::read_to_string(wire.join("build.rs")).unwrap();

    assert!(
        wire.join("schema").is_dir(),
        "the protobuf schema must live under the crate that compiles it"
    );
    assert!(
        !build.contains(".."),
        "the build script climbs out of the crate:\n{build}"
    );
}

#[test]
fn every_internal_dependency_carries_the_version_it_will_publish_as() {
    let root = checkout();
    let version = manifest(&root.join("Cargo.toml"))["workspace"]["package"]["version"]
        .as_str()
        .expect("the workspace declares the version its crates share")
        .to_string();

    let mut manifests = vec![root.join("Cargo.toml")];
    manifests.extend(crate_roots().iter().map(|krate| krate.join("Cargo.toml")));

    let mut checked = 0;
    for path in manifests {
        let manifest = manifest(&path);

        for table in dependency_tables(&manifest) {
            for (name, dependency) in table {
                // Path dependencies need matching versions for registry publication.
                if !name.starts_with("omega") || dependency.get("path").is_none() {
                    continue;
                }
                checked += 1;
                assert_eq!(
                    dependency.get("version").and_then(toml::Value::as_str),
                    Some(version.as_str()),
                    "{} depends on {name} by path without the version it publishes as",
                    path.display()
                );
            }
        }
    }

    assert!(checked > 0, "no internal dependency was found to check");
}

/// A field a crate either states itself or inherits from the workspace.
fn inherited<'a>(
    package: &'a toml::Value,
    workspace: &'a toml::Value,
    key: &str,
) -> Option<&'a toml::Value> {
    match package.get(key) {
        Some(value) if value.get("workspace").and_then(toml::Value::as_bool) == Some(true) => {
            workspace.get("workspace")?.get("package")?.get(key)
        }
        other => other,
    }
}

#[test]
fn every_published_crate_carries_the_metadata_a_registry_shows() {
    // Require publication metadata on every package.
    let workspace = manifest(&checkout().join("Cargo.toml"));

    let mut checked = 0;
    for root in crate_roots() {
        let path = root.join("Cargo.toml");
        let manifest = manifest(&path);
        let package = manifest
            .get("package")
            .expect("a crate has a package table");
        let name = package["name"].as_str().unwrap_or_default();

        for key in [
            "description",
            "license",
            "repository",
            "readme",
            "homepage",
            "keywords",
            "categories",
        ] {
            assert!(
                inherited(package, &workspace, key).is_some(),
                "{name} declares no {key}"
            );
        }

        // crates.io's own limits, which it enforces on upload and nothing
        // enforces here: at most five keywords, none over twenty characters.
        let keywords = package["keywords"]
            .as_array()
            .unwrap_or_else(|| panic!("{name}: keywords is a list"));
        assert!(
            keywords.len() <= 5,
            "{name} has {} keywords",
            keywords.len()
        );
        for keyword in keywords {
            let keyword = keyword.as_str().unwrap_or_default();
            assert!(keyword.len() <= 20, "{name}: {keyword:?} is over 20 chars");
            assert!(
                keyword.starts_with(|c: char| c.is_ascii_alphanumeric()),
                "{name}: {keyword:?} must start alphanumeric"
            );
        }

        assert!(
            !package["categories"].as_array().unwrap().is_empty(),
            "{name} declares no categories"
        );
        checked += 1;
    }

    assert!(checked > 0, "no crate was found to check");
}
