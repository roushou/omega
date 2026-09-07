//! What a published crate carries, and what it asks for.
//!
//! Cargo packages a crate's own directory and nothing above it, so a file
//! read from the repository root builds perfectly here and is simply missing
//! for everyone who installs it. Both halves of omega had that at once —
//! `omega-proto` compiled its protobuf schema from `../../schema`, and this
//! crate embedded its templates and its renderer from `../../../` — and no
//! test or build said a word, because on a developer's machine those files
//! are exactly where they are expected to be.
//!
//! These read the repository rather than a fixture, because the thing being
//! checked is where files are and what the manifests say about them.

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

/// Every `.rs` file a crate *tests* with, which `sources` deliberately skips:
/// a test is not published, so it may climb, but it still runs on somebody's
/// machine.
fn test_sources(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    walk(&root.join("tests"), &mut found);
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

/// How far each `include_str!` in this source climbs before it descends.
///
/// Only the first literal of an invocation is read, because that is where a
/// climb can be: `concat!("../", DIR, "/", NAME)` puts every `..` in the
/// first piece, and nothing a later piece expands to can bring a path back
/// up. Counting the climb rather than resolving the path is what makes a
/// macro-built include checkable at all.
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

/// One manifest, read loosely.
///
/// Not through the typed model: that one describes the manifests omega
/// *generates*, and every crate in this repository inherits half its package
/// table with `workspace = true`, which that model has no shape for.
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
    // `omega-proto` generates its types from the schema at build time, so the
    // schema has to travel with it. The same rule as the test above, asserted
    // where a build script rather than the compiler reads the path.
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
                // A path dependency with no version cannot be published:
                // cargo has nothing to write into the registry entry and
                // refuses. One with the *wrong* version publishes, and then
                // resolves to somebody else's release.
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

#[test]
fn nothing_that_runs_cargo_builds_in_the_system_temp_directory() {
    // `/tmp` is a tmpfs on most Linux installs — memory, capped at half of
    // it, wiped on reboot. That is the right home for a socket or a manifest,
    // and the wrong one for a cargo target directory, which runs to hundreds
    // of megabytes: the build is held in RAM while it runs and thrown away
    // before the next one, so every run recompiles from nothing.
    //
    // Cargo hands integration tests `CARGO_TARGET_TMPDIR` for exactly this.
    // The rule is therefore narrow: keep using `temp_dir` for scratch, but
    // not in a file that also drives a compiler.
    let mut checked = 0;

    for root in crate_roots() {
        for file in sources(&root).into_iter().chain(test_sources(&root)) {
            // This file names both needles to look for them, so it matches
            // itself and nothing else would ever pass.
            if file.ends_with(file!()) {
                continue;
            }

            let source = std::fs::read_to_string(&file).unwrap();
            if !source.contains(r#"Command::new("cargo")"#) {
                continue;
            }
            checked += 1;
            assert!(
                !source.contains("env::temp_dir"),
                "{} runs cargo and roots paths in the system temp directory —                  build somewhere on disk, such as CARGO_TARGET_TMPDIR",
                file.display()
            );
        }
    }

    assert!(checked > 0, "no cargo-running source was found to check");
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
    // Description and license decide whether a publish is accepted; readme,
    // keywords and categories decide whether anybody finds the crate
    // afterwards. All of them are invisible locally — nothing but a publish
    // reads them — so a crate added without them is only noticed on the day
    // it ships.
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
