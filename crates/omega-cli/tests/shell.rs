//! The renderer omega installs, and what keeps it in step with the daemon.
//!
//! The renderer reads the wire format the daemon writes, so the two are one
//! protocol in halves. These are the checks that make the halves inseparable:
//! everything in the tree is in the binary, the binary's version is the one
//! the shell is told, and installing leaves the directory saying only what
//! this binary carries.

use std::path::{Path, PathBuf};

use omega_cli::shell::{Installed, Renderer};

/// The checkout these tests run inside: `crates/omega-cli` → `crates` → root.
fn checkout() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("omega-cli lives two directories under the checkout")
        .to_path_buf()
}

/// A plugin directory of its own, so a test never writes where a shell looks.
fn plugins(label: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("omega-shell-{label}-{nanos}"));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn every_file_in_the_tree_is_carried_in_the_binary() {
    for renderer in Renderer::ALL {
        let source = checkout().join(renderer.source);
        let mut on_disk: Vec<String> = std::fs::read_dir(&source)
            .unwrap_or_else(|e| panic!("{}: {e}", source.display()))
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        on_disk.sort();

        let mut carried: Vec<String> = renderer
            .files
            .iter()
            .map(|asset| asset.name.to_owned())
            .collect();
        carried.sort();

        // A file added to the plugin and left out of the list would install
        // as a plugin missing a file — which loads, and draws nothing.
        assert_eq!(
            on_disk,
            carried,
            "{} is not the set of files in {}",
            renderer.id,
            source.display()
        );
    }
}

#[test]
fn the_renderer_tells_the_shell_the_version_this_binary_speaks() {
    for renderer in Renderer::ALL {
        // Not patched at install time: the file that ships is the file that
        // is written, so a `--link` install serves the same version a copy
        // does. This is what keeps the two honest.
        assert_eq!(
            renderer.declared_version().as_deref(),
            Some(Renderer::VERSION),
            "{}'s manifest announces a version omega does not carry",
            renderer.id
        );
    }
}

#[test]
fn a_fresh_install_is_current() {
    let plugins = plugins("fresh");
    let renderer = &Renderer::VIEW;

    assert_eq!(renderer.installed(&plugins), Installed::Missing);
    renderer.install(&plugins).unwrap();
    assert_eq!(renderer.installed(&plugins), Installed::Current);
}

#[test]
fn installing_takes_away_what_an_older_renderer_left() {
    let plugins = plugins("strays");
    let renderer = &Renderer::VIEW;
    let dir = renderer.install(&plugins).unwrap();

    // A file an earlier version shipped. The shell loads the directory, not
    // omega's list, so leaving it behind leaves it running.
    std::fs::write(dir.join("Legacy.qml"), "Item {}").unwrap();
    assert!(matches!(
        renderer.installed(&plugins),
        Installed::Stale { .. }
    ));

    renderer.install(&plugins).unwrap();
    assert!(!dir.join("Legacy.qml").exists());
    assert_eq!(renderer.installed(&plugins), Installed::Current);
}

#[test]
fn an_install_this_binary_did_not_write_reads_as_stale() {
    let plugins = plugins("edited");
    let renderer = &Renderer::VIEW;
    let dir = renderer.install(&plugins).unwrap();

    std::fs::write(dir.join("Props.js"), "// somebody's own\n").unwrap();

    match renderer.installed(&plugins) {
        // Still announcing this version while reading a different one is
        // exactly the failure the check exists for: the manifest is not
        // evidence, the contents are.
        Installed::Stale { version } => assert_eq!(version.as_deref(), Some(Renderer::VERSION)),
        other => panic!("an edited install read as {other:?}"),
    }
}

#[test]
fn a_link_says_where_the_shell_is_drawing_from() {
    let plugins = plugins("linked");
    let renderer = &Renderer::VIEW;
    let checkout = checkout();

    renderer.link(&plugins, &checkout).unwrap();

    assert_eq!(
        renderer.installed(&plugins),
        Installed::Linked(checkout.join(renderer.source)),
        "a linked renderer must not read as a copy: what it draws is whatever is in that tree"
    );
}

#[test]
fn installing_over_a_link_leaves_no_link_behind() {
    let plugins = plugins("relink");
    let renderer = &Renderer::VIEW;

    renderer.link(&plugins, &checkout()).unwrap();
    let dir = renderer.install(&plugins).unwrap();

    assert!(!dir.is_symlink());
    assert_eq!(renderer.installed(&plugins), Installed::Current);
}

#[test]
fn uninstall_leaves_a_plugin_that_is_not_ours_alone() {
    let plugins = plugins("foreign");
    let renderer = &Renderer::VIEW;

    // Somebody else's plugin, sitting under the name omega would use.
    let dir = renderer.dir_in(&plugins);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("manifest.json"),
        r#"{"id": "someone.else", "kinds": ["bar-widget"]}"#,
    )
    .unwrap();

    let refused = renderer.uninstall(&plugins).unwrap_err();
    assert!(
        refused.to_string().contains("somebody else's plugin"),
        "{refused}"
    );
    assert!(dir.join("manifest.json").exists());
}

#[test]
fn uninstall_takes_away_what_omega_installed() {
    let plugins = plugins("gone");
    let renderer = &Renderer::VIEW;
    renderer.install(&plugins).unwrap();

    assert!(renderer.uninstall(&plugins).unwrap().is_some());
    assert_eq!(renderer.installed(&plugins), Installed::Missing);

    // Removing what is not there is not a failure, and says so.
    assert!(renderer.uninstall(&plugins).unwrap().is_none());
}

#[test]
fn a_copy_that_claims_the_right_version_still_says_what_is_wrong() {
    let plugins = plugins("phrasing");
    let renderer = &Renderer::VIEW;
    let dir = renderer.install(&plugins).unwrap();

    // The manifest still announces this version, because only the contents
    // changed. Reporting the two versions here would print the same number
    // twice and read as a bug in the check rather than a fact about the disk.
    std::fs::write(dir.join("Props.js"), "// somebody's own\n").unwrap();
    assert_eq!(
        renderer.installed(&plugins).difference().as_deref(),
        Some("edited since it was installed")
    );

    let manifest = dir.join("manifest.json");
    let older = std::fs::read_to_string(&manifest)
        .unwrap()
        .replace(Renderer::VERSION, "0.0.9");
    std::fs::write(&manifest, older).unwrap();
    assert_eq!(
        renderer.installed(&plugins).difference(),
        Some(format!(
            "0.0.9 installed, this omega draws {}",
            Renderer::VERSION
        ))
    );
}

#[test]
fn what_matches_has_no_difference_to_report() {
    let plugins = plugins("quiet");
    let renderer = &Renderer::VIEW;

    // `omega check` warns on the difference and nothing else, so a missing
    // renderer must not report one: a machine that puts nothing on its bar is
    // entitled to have none installed.
    assert_eq!(renderer.installed(&plugins).difference(), None);
    renderer.install(&plugins).unwrap();
    assert_eq!(renderer.installed(&plugins).difference(), None);
    renderer.link(&plugins, &checkout()).unwrap();
    assert_eq!(renderer.installed(&plugins).difference(), None);
}
