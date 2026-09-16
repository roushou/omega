//! Embedded renderer assets, version matching, and installation integrity tests.

use std::path::{Path, PathBuf};

use omega_omarchy::{Installed, Renderer};
use omega_renderer::{Core, Icons};

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
        let on_disk =
            Renderer::walk(&source).unwrap_or_else(|| panic!("cannot read {}", source.display()));

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
        // Linked and copied installs use the same embedded manifest.
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
    TestInstallation::install(renderer, &plugins).unwrap();
    assert_eq!(renderer.installed(&plugins), Installed::Current);
}

#[test]
fn installing_takes_away_what_an_older_renderer_left() {
    let plugins = plugins("strays");
    let renderer = &Renderer::VIEW;
    let dir = TestInstallation::install(renderer, &plugins).unwrap();

    // A file an earlier version shipped. The shell loads the directory, not
    // omega's list, so leaving it behind leaves it running.
    std::fs::write(dir.join("Legacy.qml"), "Item {}").unwrap();
    assert!(matches!(
        renderer.installed(&plugins),
        Installed::Stale { .. }
    ));

    TestInstallation::install(renderer, &plugins).unwrap();
    assert!(!dir.join("Legacy.qml").exists());
    assert_eq!(renderer.installed(&plugins), Installed::Current);
}

#[test]
fn an_install_this_binary_did_not_write_reads_as_stale() {
    let plugins = plugins("edited");
    let renderer = &Renderer::VIEW;
    let dir = TestInstallation::install(renderer, &plugins).unwrap();

    std::fs::write(dir.join("Props.js"), "// somebody's own\n").unwrap();

    match renderer.installed(&plugins) {
        // Detect content changes even when the manifest version matches.
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
    let dir = TestInstallation::install(renderer, &plugins).unwrap();

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
    TestInstallation::install(renderer, &plugins).unwrap();

    assert!(renderer.uninstall(&plugins).unwrap().is_some());
    assert_eq!(renderer.installed(&plugins), Installed::Missing);

    // Removing what is not there is not a failure, and says so.
    assert!(renderer.uninstall(&plugins).unwrap().is_none());
}

#[test]
fn a_copy_that_claims_the_right_version_still_says_what_is_wrong() {
    let plugins = plugins("phrasing");
    let renderer = &Renderer::VIEW;
    let dir = TestInstallation::install(renderer, &plugins).unwrap();

    // Equal versions with different contents must report a content mismatch.
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

    // A missing optional renderer is not a version mismatch.
    assert_eq!(renderer.installed(&plugins).difference(), None);
    TestInstallation::install(renderer, &plugins).unwrap();
    assert_eq!(renderer.installed(&plugins).difference(), None);
    renderer.link(&plugins, &checkout()).unwrap();
    assert_eq!(renderer.installed(&plugins).difference(), None);
}

#[test]
fn the_checked_in_icon_set_is_what_the_table_generates() {
    let path = checkout()
        .join("crates/omega-renderer/shell/core")
        .join(Icons::FILE);
    let generated = Icons::generate();

    if std::env::var_os("OMEGA_REGENERATE").is_some() {
        std::fs::write(&path, &generated).expect("the shell tree is writable");
        return;
    }

    let on_disk = std::fs::read_to_string(&path).expect("Icons.js is checked in");
    assert_eq!(
        on_disk,
        generated,
        "{} is not what the icon table generates. \
         Run `OMEGA_REGENERATE=1 cargo test -p omega-renderer`.",
        Icons::FILE
    );
}

#[test]
fn the_shell_carries_the_icon_set_the_table_declares() {
    // Verify embedded assets as well as source files.
    let icons = Core::FILES
        .iter()
        .find(|asset| asset.name == format!("core/{}", Icons::FILE))
        .expect("the view renderer carries an icon set");

    for glyph in omega_proto::Glyph::ALL {
        assert!(
            icons.contents.contains(&format!("{:?}:", glyph.name())),
            "the table declares {glyph} and the installed icon set has no glyph for it"
        );
    }
}

#[test]
fn installed_connection_embeds_the_full_bundle_identity_and_linked_sources_do_not() {
    let plugins = tempfile::tempdir().unwrap();
    let renderer = Renderer::VIEW;
    let directory = TestInstallation::install(&renderer, plugins.path()).unwrap();
    let connection =
        std::fs::read_to_string(directory.join("core/RendererConnection.qml")).unwrap();
    assert!(connection.contains(renderer.build().fingerprint()));
    assert_eq!(renderer.installed(plugins.path()), Installed::Current);
    renderer.link(plugins.path(), &checkout()).unwrap();
    let connection =
        std::fs::read_to_string(directory.join("core/RendererConnection.qml")).unwrap();
    assert!(connection.contains("readonly property string buildFingerprint: \"\""));
}

struct TestInstallation;

impl TestInstallation {
    fn install(
        renderer: &omega_omarchy::Renderer,
        plugins: &std::path::Path,
    ) -> anyhow::Result<std::path::PathBuf> {
        let layout = omega_host::Layout::at(
            plugins.join("config"),
            plugins.join("state"),
            plugins.join("cache"),
        );
        renderer
            .install(plugins, &omega_host::recovery::RecoveryStore::new(&layout))
            .map(|installed| installed.target)
    }
}
