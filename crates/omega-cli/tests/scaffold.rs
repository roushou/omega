//! What `omega init` generates, checked against the invariants that make a
//! generated workspace build.

use omega_cli::checkout::SourceTree;
use omega_cli::scaffold::{PluginName, Published, Scaffold, Template};
use omega_host::Toml;
use omega_host::workspace::cargo::{CargoManifest, Edition};
use omega_proto::UnitName;

fn unit() -> UnitName {
    UnitName::parse("battery-widget").unwrap()
}

#[test]
fn every_member_inherits_from_the_workspace_and_never_declares() {
    let scaffold = Scaffold::new();
    let workspace = scaffold.workspace_manifest();
    let declared: Vec<&str> = workspace
        .workspace
        .as_ref()
        .unwrap()
        .dependencies
        .names()
        .collect();

    for member in [
        scaffold.unit_crate_manifest(&unit()),
        scaffold.system_manifest(),
    ] {
        for (name, dependency) in member.dependencies.iter() {
            // A member of this workspace is reached by path and has no
            // version to inherit; everything else inherits one.
            if dependency.path().is_some() {
                continue;
            }
            assert!(
                dependency.is_inherited(),
                "{name} declares a version of its own: {member:?}"
            );
            assert!(
                declared.contains(&name.as_str()),
                "{name} is inherited but the workspace declares no version for it"
            );
        }
    }
}

#[test]
fn a_plugin_depends_on_one_crate() {
    let scaffold = Scaffold::new();
    let unit_manifest = scaffold.unit_crate_manifest(&unit());
    let names: Vec<&str> = unit_manifest.dependencies.names().collect();

    // A plugin holds handles, draws a view, and returns `omega::Result`. The
    // protocol, the runtime and the manifest are all behind that one name.
    assert_eq!(names, vec!["omega"]);
}

#[test]
fn a_unit_and_the_config_plane_depend_on_different_things() {
    let scaffold = Scaffold::new();
    let unit_manifest = scaffold.unit_crate_manifest(&unit());
    let system_manifest = scaffold.system_manifest();
    let unit_crate: Vec<&str> = unit_manifest.dependencies.names().collect();
    let system: Vec<&str> = system_manifest.dependencies.names().collect();

    // A plugin is written against the plugin crate, and nothing else: no
    // runtime to start, no protocol to speak, no manifest to keep.
    assert_eq!(unit_crate, vec!["omega"], "{unit_crate:?}");

    // The config plane computes a document and exits, so it needs neither the
    // runtime nor the protocol — and a plugin never touches the authoring API.
    assert!(!system.contains(&"omega"), "{system:?}");
    assert!(system.contains(&"omega-document"), "{system:?}");
    assert!(!unit_crate.contains(&"omega-document"), "{unit_crate:?}");
}

#[test]
fn the_workspace_manifest_is_a_workspace_cargo_would_accept() {
    let manifest = Scaffold::new().workspace_manifest();
    let workspace = manifest.workspace.as_ref().unwrap();

    assert_eq!(workspace.resolver.as_deref(), Some("3"));
    assert_eq!(
        workspace.package.as_ref().unwrap().edition.as_deref(),
        Some("2024")
    );
    // The config plane is a member alongside the units it configures.
    assert_eq!(workspace.members, vec!["system".to_string()]);
    assert!(
        manifest.package.is_none(),
        "a workspace root is not a package"
    );

    let encoded = Toml::encode(&manifest).unwrap();
    assert_eq!(Toml::decode::<CargoManifest>(&encoded).unwrap(), manifest);
}

#[test]
fn the_config_plane_is_scaffolded_as_its_own_crate() {
    let scaffold = Scaffold::new();
    let system = scaffold.system_manifest();

    assert_eq!(system.package.as_ref().unwrap().name, "system");
    assert_eq!(
        system.package.as_ref().unwrap().edition,
        Edition::inherited()
    );
    // The authoring vocabulary and nothing else: a config says what the
    // machine should be and never names the protocol.
    assert_eq!(
        system.dependencies.names().collect::<Vec<_>>(),
        vec!["omega-document", "omega-omarchy"]
    );

    let main = scaffold.system_main();
    assert!(main.contains("Document::"), "the template emits a document");
    assert!(main.contains(".emit()"));
}

#[test]
fn a_founded_config_names_no_plugin() {
    let main = Scaffold::new().system_main();

    // The initial workspace must contain only scaffolded members.
    assert!(
        !main.contains("{unit"),
        "a token was left unstamped: {main}"
    );
    assert!(!main.contains("battery"), "{main}");
    assert!(
        !main.contains("PluginWidget::"),
        "the initial layout contains only native widgets: {main}"
    );
}

#[test]
fn the_config_plane_reaches_a_plugin_by_path() {
    // Scaffolding adds a system dependency on the plugin library.
    let dependency = Scaffold::depends_on(&unit());

    assert!(
        dependency
            .path()
            .is_some_and(|path| path.ends_with("plugins/battery-widget")),
        "{dependency:?}"
    );
    assert!(
        !dependency.is_inherited(),
        "a crate in this workspace has no version to inherit: {dependency:?}"
    );
}

#[test]
fn the_unit_crate_is_named_after_the_unit() {
    let unit_crate = Scaffold::new().unit_crate_manifest(&unit());
    let package = unit_crate.package.as_ref().unwrap();

    // The build copies `target/release/<crate name>`, so the crate name and
    // the unit name must be the same string.
    assert_eq!(package.name, unit().as_str());
    assert_eq!(package.edition, Edition::inherited());
}

#[test]
fn the_bundled_plugin_declares_by_holding() {
    let main = Template::Minimal.library();

    // The shortest useful plugin: hold what you need, draw what you know.
    assert!(main.contains("omega::Surface"), "{main}");
    assert!(main.contains("impl Surface for"), "{main}");
    assert!(main.contains("omega::plugin!()"), "{main}");

    // Nothing declared twice, and nothing the author has to run themselves:
    // no manifest to include, no surface name to repeat, no runtime to start.
    assert!(!main.contains("include_str!"), "{main}");
    assert!(!main.contains("SURFACE"), "{main}");
    assert!(!main.contains("tokio::main"), "{main}");
    assert!(!main.contains("loop {"), "{main}");
}

#[test]
fn the_program_is_the_library_and_a_call() {
    let main = Scaffold::new()
        .unit_main(&PluginName::parse(unit().as_str()).unwrap())
        .unwrap();

    // A plugin is a library so the config plane can depend on it. What is
    // left in the program is the call that runs what the library declared.
    assert!(main.contains("battery_widget::plugin().run()"), "{main}");
    assert!(!main.contains("Surface"), "{main}");
}

#[test]
fn the_scaffolded_unit_arrives_with_tests_that_pass() {
    let main = Template::Minimal.library();

    // Generated plugins must include executable fixture tests.
    assert!(main.contains("#[cfg(test)]"), "{main}");
    assert!(main.contains("omega::testing"), "{main}");
    assert!(main.contains("Drawn::of"), "{main}");
    assert!(
        !main.contains("{unit}"),
        "the token was left unstamped: {main}"
    );
}

#[test]
fn the_program_uses_the_rust_crate_identifier() {
    let scaffold = Scaffold::new();
    // Rust spells `battery-widget` as `battery_widget`, and the program has
    // to say the name the way the language does.
    let hyphenated = UnitName::parse("battery-widget").unwrap();
    assert!(
        scaffold
            .unit_main(&PluginName::parse(hyphenated.as_str()).unwrap())
            .unwrap()
            .contains("battery_widget::plugin()"),
        "the program should reach its library by its Rust name"
    );
}

#[test]
fn a_config_names_the_published_crates_whoever_scaffolded_it() {
    let manifest = Scaffold::from_source(Published::at("0.4.2")).workspace_manifest();
    let dependencies = &manifest.workspace.as_ref().unwrap().dependencies;

    // Generated manifests use portable registry dependency requirements.
    for name in ["omega", "omega-document"] {
        let dependency = dependencies.get(name).unwrap();
        assert_eq!(dependency.version(), Some("0.4.2"), "{name}");
        assert!(dependency.path().is_none(), "{name} resolved to a path");
        assert!(
            dependency.repository().is_none(),
            "{name} resolved to a repo"
        );
    }

    // The SDK dependency alias omega must name the registry package omega-rs.
    let sdk = dependencies.get("omega").unwrap();
    assert_eq!(sdk.package(), Some("omega-rs"));
    assert!(
        dependencies.get("omega-rs").is_none(),
        "the table is keyed by what a config calls it, not what the registry does"
    );

    // Registry dependencies do not care where omega comes from.
    assert!(dependencies.get("anyhow").is_none());

    let encoded = Toml::encode(&manifest).unwrap();
    assert_eq!(Toml::decode::<CargoManifest>(&encoded).unwrap(), manifest);
}

#[test]
fn a_checkout_is_an_override_rather_than_a_manifest() {
    let tree = SourceTree::detect()
        .unwrap()
        .expect("the tests run from a checkout");
    let patched = tree.patch(false).unwrap();

    // Local patches are gitignored and keyed by registry package name, including omega-rs.
    for name in ["omega-rs", "omega-document"] {
        let path = patched.get(name).unwrap().path().unwrap();
        assert!(std::path::Path::new(path).is_absolute(), "{name}: {path}");
        assert!(
            std::path::Path::new(path).join("Cargo.toml").exists(),
            "{name}: {path}"
        );
    }
    assert!(
        patched.get("omega").is_none(),
        "a patch keyed by the manifest's name would not match the registry"
    );

    assert_eq!(tree.version().unwrap(), env!("CARGO_PKG_VERSION"));
    assert!(Scaffold::new().gitignore().contains("/.cargo/"));
}

#[test]
fn a_checkout_cargo_owns_is_not_one_to_link() {
    // Durable checkouts can be linked regardless of profile; Cargo-managed checkout caches cannot.
    let cargo_home = std::path::PathBuf::from(
        std::env::var("CARGO_HOME")
            .unwrap_or_else(|_| format!("{}/.cargo", std::env::var("HOME").unwrap())),
    );
    let transient = cargo_home.join("git/checkouts/omega-1234/abcdef");

    assert!(
        SourceTree::at(&transient).is_err(),
        "a tree cargo owns is not a checkout to point a config at"
    );
    assert!(
        SourceTree::detect().unwrap().is_some(),
        "the tests run from a checkout somebody keeps"
    );
}

#[test]
fn a_path_that_is_not_a_checkout_says_so() {
    let error = SourceTree::at("/etc").unwrap_err().to_string();

    // Reject nonexistent checkout paths before writing Cargo patches.
    assert!(error.contains("not an omega checkout"), "{error}");
}

#[test]
fn the_line_omega_new_prints_names_only_things_the_plugin_has() {
    let hint = Scaffold::placement_hint(
        &PluginName::parse(unit().as_str()).unwrap(),
        Template::Minimal,
    );
    let lib = Template::Minimal.library();

    // Placement hints must reference exported plugin symbols.
    assert!(hint.contains("battery_widget::Hello"), "{hint}");
    assert!(lib.contains("pub struct Hello"), "{lib}");
    assert!(!hint.contains("Settings"), "{hint}");

    // Fully qualified: a hint that needs a second hint about an import is a
    // hint that failed.
    assert!(
        hint.contains("omega_omarchy::shell::PluginWidget::new"),
        "{hint}"
    );
}
