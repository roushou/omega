use super::*;
use crate::{
    checkout::{CheckoutLink, SourceTree},
    scaffold::Template,
};
use omega_host::{AtomicFile, TempPath};
use std::path::PathBuf;

struct Fixture {
    root: PathBuf,
    layout: Layout,
}

impl Fixture {
    fn new() -> Self {
        let root = TempPath::sibling(&std::env::temp_dir().join("omega-scaffold"), "test");
        let layout = Layout::at(root.join("config"), root.join("state"), root.join("cache"));
        Self { root, layout }
    }

    fn open(&self) -> ConfigWorkspace {
        ConfigWorkspace::open(self.layout.clone()).unwrap()
    }
    fn init(&self) {
        self.open()
            .prepare_init(InitialShell::Default)
            .unwrap()
            .apply()
            .unwrap();
    }
    fn write(&self, path: PathBuf, source: &str) {
        AtomicFile::at(path).write(source.as_bytes()).unwrap();
    }
    fn root_source(&self) -> String {
        std::fs::read_to_string(self.layout.workspace_manifest()).unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[test]
fn invalid_system_manifest_creates_no_plugin_or_manifest_edits() {
    let f = Fixture::new();
    f.init();
    f.write(f.layout.system_manifest(), "[broken");
    let before = f.root_source();
    let name = PluginName::parse("hello").unwrap();
    let path = f.layout.unit_src_dir(name.unit());
    assert!(f.open().prepare_plugin(name, Template::Minimal).is_err());
    assert!(!path.exists());
    assert_eq!(before, f.root_source());
}

#[test]
fn names_must_work_as_rust_crates() {
    for name in ["type", "gen", "self", "system", "omega", "proc-macro"] {
        assert!(PluginName::parse(name).is_err(), "{name}");
    }
    assert_eq!(
        PluginName::parse("audio-output").unwrap().rust_ident(),
        "audio_output"
    );
}

#[test]
fn new_preserves_comments_options_and_inherited_package_fields() {
    let f = Fixture::new();
    f.init();
    let root = f
        .root_source()
        .replace(
            "edition = \"2024\"",
            "# chosen edition\nedition = \"2021\"\nversion = \"1.0.0\"",
        )
        .replace(
            &format!("omega-document = {:?}", env!("CARGO_PKG_VERSION")),
            &format!(
                "omega-document = {{ version = {:?}, default-features = false }} # keep",
                env!("CARGO_PKG_VERSION")
            ),
        );
    f.write(f.layout.workspace_manifest(), &root);
    let system = "# my config\n[package]\nname = \"system\"\nversion.workspace = true\nedition.workspace = true\n\n[dependencies]\nomega-document.workspace = true # authoring\n";
    f.write(f.layout.system_manifest(), system);
    f.open()
        .prepare_plugin(PluginName::parse("hello").unwrap(), Template::Minimal)
        .unwrap()
        .apply()
        .unwrap();
    let after = f.root_source();
    assert!(after.contains("# chosen edition\nedition = \"2021\""));
    assert!(after.contains("default-features = false } # keep"));
    let system = std::fs::read_to_string(f.layout.system_manifest()).unwrap();
    assert!(system.starts_with("# my config"));
    assert!(system.contains("version.workspace = true"));
    assert!(system.contains("omega-document.workspace = true # authoring"));
}

#[test]
fn excluded_plugins_are_refused_before_writing() {
    let f = Fixture::new();
    f.init();
    let root = f
        .root_source()
        .replace("[workspace]\n", "[workspace]\nexclude = [\"units/*\"]\n");
    f.write(f.layout.workspace_manifest(), &root);
    let error = f
        .open()
        .prepare_plugin(PluginName::parse("hello").unwrap(), Template::Minimal)
        .unwrap_err();
    assert!(error.to_string().contains("excluded"));
    assert_eq!(f.root_source(), root);
    assert!(!f.layout.config.join("units/hello").exists());
}

#[test]
fn existing_membership_globs_cover_a_plugin_before_its_directory_exists() {
    let f = Fixture::new();
    f.init();
    let root = f.root_source().replace(
        "members = [\"system\"]",
        "members = [\"system\", \"units/*\"]",
    );
    f.write(f.layout.workspace_manifest(), &root);
    f.open()
        .prepare_plugin(PluginName::parse("hello").unwrap(), Template::Minimal)
        .unwrap()
        .apply()
        .unwrap();
    assert_eq!(f.root_source(), root);
}

#[test]
fn rust_dependency_alias_collisions_do_not_replace_dependencies() {
    let f = Fixture::new();
    f.init();
    let system =
        std::fs::read_to_string(f.layout.system_manifest()).unwrap() + "audio_output = \"1\"\n";
    f.write(f.layout.system_manifest(), &system);
    assert!(
        f.open()
            .prepare_plugin(
                PluginName::parse("audio-output").unwrap(),
                Template::Minimal
            )
            .is_err()
    );
    assert_eq!(
        std::fs::read_to_string(f.layout.system_manifest()).unwrap(),
        system
    );
}

#[test]
fn normalized_workspace_package_collisions_are_refused() {
    let f = Fixture::new();
    f.init();
    f.open()
        .prepare_plugin(
            PluginName::parse("audio-output").unwrap(),
            Template::Minimal,
        )
        .unwrap()
        .apply()
        .unwrap();
    assert!(
        f.open()
            .prepare_plugin(
                PluginName::parse("audio_output").unwrap(),
                Template::Minimal
            )
            .is_err()
    );
}

#[test]
fn stale_preparation_does_not_overwrite_an_editors_changes() {
    let f = Fixture::new();
    f.init();
    let workspace = f.open();
    let prepared = workspace
        .prepare_plugin(PluginName::parse("hello").unwrap(), Template::Minimal)
        .unwrap();
    let updated = f.root_source() + "\n# edited while command was preparing\n";
    f.write(f.layout.workspace_manifest(), &updated);
    assert!(prepared.apply().is_err());
    assert_eq!(f.root_source(), updated);
    assert!(!f.layout.config.join("units/hello").exists());
}

#[test]
fn destination_created_after_preparation_is_preserved_and_manifests_rolled_back() {
    let f = Fixture::new();
    f.init();
    let before = f.root_source();
    let system = std::fs::read_to_string(f.layout.system_manifest()).unwrap();
    let workspace = f.open();
    let prepared = workspace
        .prepare_plugin(PluginName::parse("hello").unwrap(), Template::Minimal)
        .unwrap();
    let marker = f.layout.config.join("units/hello/mine");
    f.write(marker.clone(), "my file");
    assert!(prepared.apply().is_err());
    assert_eq!(f.root_source(), before);
    assert_eq!(
        std::fs::read_to_string(f.layout.system_manifest()).unwrap(),
        system
    );
    assert_eq!(std::fs::read_to_string(marker).unwrap(), "my file");
}

#[test]
fn repeated_init_preserves_user_files_and_missing_entrypoint_can_be_repaired() {
    let f = Fixture::new();
    f.init();
    f.write(f.layout.gitignore(), "# private files\n/secrets\n/target\n");
    f.write(f.layout.system_main(), "// my system\n");
    f.init();
    assert_eq!(
        std::fs::read_to_string(f.layout.gitignore()).unwrap(),
        "# private files\n/secrets\n/target\n/.cargo/\n"
    );
    assert_eq!(
        std::fs::read_to_string(f.layout.system_main()).unwrap(),
        "// my system\n"
    );
    std::fs::remove_file(f.layout.system_main()).unwrap();
    f.init();
    assert!(
        std::fs::read_to_string(f.layout.system_main())
            .unwrap()
            .contains("Document")
    );
}

#[test]
fn missing_sdk_dependency_is_added_but_wrong_package_is_refused() {
    let f = Fixture::new();
    f.init();
    let root = f.root_source();
    let mut doc: toml_edit::DocumentMut = root.parse().unwrap();
    doc["workspace"]["dependencies"]
        .as_table_mut()
        .unwrap()
        .remove("omega");
    f.write(f.layout.workspace_manifest(), &doc.to_string());
    f.open()
        .prepare_plugin(PluginName::parse("hello").unwrap(), Template::Minimal)
        .unwrap()
        .apply()
        .unwrap();
    assert!(f.root_source().contains("package = \"omega-rs\""));
    f.write(
        f.layout.workspace_manifest(),
        &root.replace("package = \"omega-rs\"", "package = \"other\""),
    );
    assert!(
        f.open()
            .prepare_plugin(PluginName::parse("another").unwrap(), Template::Minimal)
            .is_err()
    );
}

#[test]
fn linking_preserves_other_patches_and_dependency_options() {
    let f = Fixture::new();
    f.init();
    f.write(f.layout.cargo_config(), "# compiler options\n[build]\njobs = 2\n[patch.crates-io]\nother = { path = \"/somewhere\" } # unrelated\n");
    let root = f.root_source().replace(
        "package = \"omega-rs\"",
        "package = \"omega-rs\", default-features = false",
    );
    f.write(f.layout.workspace_manifest(), &root);
    let workspace = f.open();
    CheckoutLink::new(&workspace)
        .prepare(Some(&SourceTree::detect().unwrap().unwrap()))
        .unwrap()
        .apply()
        .unwrap();
    assert!(f.root_source().contains("default-features = false"));
    CheckoutLink::new(&workspace)
        .prepare(None)
        .unwrap()
        .apply()
        .unwrap();
    let config = std::fs::read_to_string(f.layout.cargo_config()).unwrap();
    assert!(config.contains("other = { path = \"/somewhere\" } # unrelated"));
    assert!(config.starts_with("# compiler options"));
    assert!(!config.contains("omega-rs"));
}

#[test]
fn retry_completes_manifest_references_left_before_plugin_publication() {
    let f = Fixture::new();
    f.init();
    let root = f.root_source().replace(
        "members = [\"system\"]",
        "members = [\"system\", \"units/hello\"]",
    );
    f.write(f.layout.workspace_manifest(), &root);
    let system = std::fs::read_to_string(f.layout.system_manifest()).unwrap()
        + "hello = { path = \"../units/hello\" }\n";
    f.write(f.layout.system_manifest(), &system);
    f.open()
        .prepare_plugin(PluginName::parse("hello").unwrap(), Template::Minimal)
        .unwrap()
        .apply()
        .unwrap();
    assert_eq!(f.root_source(), root);
    assert_eq!(
        std::fs::read_to_string(f.layout.system_manifest()).unwrap(),
        system
    );
    assert!(f.layout.config.join("units/hello/src/lib.rs").is_file());
}

#[test]
fn rollback_reports_external_changes_and_restores_other_files() {
    let f = Fixture::new();
    f.init();
    let before = f.root_source();
    let mut root = FileEdit::read(f.layout.workspace_manifest()).unwrap();
    root.replace(before.clone() + "\n# operation\n");
    let mut ignore = FileEdit::read(f.layout.gitignore()).unwrap();
    ignore.replace("/changed\n".into());
    let edits = FileEdits::new(vec![root, ignore]);
    edits.apply().unwrap();
    f.write(f.layout.gitignore(), "# editor changed this\n");
    let error = edits.rollback(anyhow::anyhow!("publication failed"));
    assert!(error.to_string().contains("incomplete rollback"));
    assert!(error.to_string().contains(".gitignore"));
    assert_eq!(f.root_source(), before);
    assert_eq!(
        std::fs::read_to_string(f.layout.gitignore()).unwrap(),
        "# editor changed this\n"
    );
}
