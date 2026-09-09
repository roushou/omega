//! What `omega init` generates, checked against the invariants that make a
//! generated workspace build.

use omega_cli::scaffold::{Published, Scaffold, SourceTree};
use omega_daemon::host::cargo::CargoManifest;
use omega_host::Toml;
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
    // The config plane is a member alongside the units it configures.
    assert_eq!(
        workspace.members,
        vec!["system".to_string(), "units/*".to_string()]
    );
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
    // The authoring vocabulary and nothing else: a config says what the
    // machine should be and never names the protocol.
    assert_eq!(
        system.dependencies.names().collect::<Vec<_>>(),
        vec!["anyhow", "omega-document"]
    );

    let main = scaffold.system_main();
    assert!(main.contains("Document::"), "the template emits a document");
    assert!(main.contains(".emit()"));
}

#[test]
fn a_founded_config_names_no_plugin() {
    let main = Scaffold::new().system_main();

    // `omega init` founds a config; `omega new` writes plugins. A template
    // that named one would name a crate the workspace does not build, and
    // the first `omega build` on a fresh machine would fail.
    assert!(
        !main.contains("{unit"),
        "a token was left unstamped: {main}"
    );
    assert!(!main.contains("battery"), "{main}");
    assert!(main.contains("vec![]"), "the bar starts empty: {main}");
}

#[test]
fn the_config_plane_reaches_a_plugin_by_path() {
    // What `omega new` adds, and the reason it does: depending on the plugin
    // is what makes its settings a type here rather than a map of strings.
    let dependency = Scaffold::depends_on(&unit());

    assert!(
        dependency
            .path()
            .is_some_and(|path| path.ends_with("units/battery-widget")),
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
    assert_eq!(package.edition, "2024");
}

#[test]
fn the_bundled_plugin_declares_by_holding() {
    let main = Scaffold::new().unit_lib(&unit()).unwrap();

    // The shortest useful plugin: hold what you need, draw what you know.
    assert!(main.contains("#[derive(omega::Widget)]"), "{main}");
    assert!(main.contains("impl Widget for"), "{main}");
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
    let main = Scaffold::new().unit_main(&unit()).unwrap();

    // A plugin is a library so the config plane can depend on it. What is
    // left in the program is the call that runs what the library declared.
    assert!(main.contains("battery_widget::plugin().run()"), "{main}");
    assert!(!main.contains("Widget"), "{main}");
}

#[test]
fn the_scaffolded_unit_arrives_with_tests_that_pass() {
    let main = Scaffold::new().unit_lib(&unit()).unwrap();

    // A plugin nobody can test is a plugin nobody changes twice. The
    // shortest way to say one is testable is to hand someone one that is.
    assert!(main.contains("#[cfg(test)]"), "{main}");
    assert!(main.contains("omega::testing"), "{main}");
    assert!(main.contains("Drawn::of"), "{main}");
    assert!(
        !main.contains("{unit}"),
        "the token was left unstamped: {main}"
    );
}

#[test]
fn the_scaffolded_unit_names_itself_in_the_command_that_runs_it() {
    let scaffolded = UnitName::parse("clock").unwrap();
    let scaffold = Scaffold::new();

    assert!(
        scaffold
            .unit_lib(&scaffolded)
            .unwrap()
            .contains("omega dev clock"),
        "the plugin should say how to run it"
    );
    // Rust spells `battery-widget` as `battery_widget`, and the program has
    // to say the name the way the language does.
    let hyphenated = UnitName::parse("battery-widget").unwrap();
    assert!(
        scaffold
            .unit_main(&hyphenated)
            .unwrap()
            .contains("battery_widget::plugin()"),
        "the program should reach its library by its Rust name"
    );
}

#[test]
fn a_config_names_the_published_crates_whoever_scaffolded_it() {
    let manifest = Scaffold::from_source(Published::at("0.4.2")).workspace_manifest();
    let dependencies = &manifest.workspace.as_ref().unwrap().dependencies;

    // One shape, always. A config is a git repository that has to build on
    // every machine it is cloned onto, and a path into somebody's home
    // directory does not travel — so no manifest omega writes contains one.
    for name in ["omega", "omega-document"] {
        let dependency = dependencies.get(name).unwrap();
        assert_eq!(dependency.version(), Some("0.4.2"), "{name}");
        assert!(dependency.path().is_none(), "{name} resolved to a path");
        assert!(
            dependency.repository().is_none(),
            "{name} resolved to a repo"
        );
    }

    // The SDK is published as `omega-rs`, because `omega` on crates.io is an
    // unrelated crate — but a config says `omega`, and every template, doc
    // and `use` in the world says `omega`. Cargo's `package` key is what
    // holds those apart, and it has to be in the manifest omega writes or the
    // config resolves to somebody else's crate.
    let sdk = dependencies.get("omega").unwrap();
    assert_eq!(sdk.package(), Some("omega-rs"));
    assert!(
        dependencies.get("omega-rs").is_none(),
        "the table is keyed by what a config calls it, not what the registry does"
    );

    // Registry dependencies do not care where omega comes from.
    assert_eq!(dependencies.get("anyhow").unwrap().version(), Some("1"));

    let encoded = Toml::encode(&manifest).unwrap();
    assert_eq!(Toml::decode::<CargoManifest>(&encoded).unwrap(), manifest);
}

#[test]
fn a_checkout_is_an_override_rather_than_a_manifest() {
    let tree = SourceTree::detect().expect("the tests run from a checkout");
    let patched = tree.patch().unwrap();

    // Building against a checkout is cargo's `[patch]`, in a file the
    // scaffold tells git to ignore — so the committed manifest stays the
    // same on the machine that develops omega and the machine that only
    // runs it.
    // Keyed by what the registry calls a crate, not what the manifest does:
    // `[patch.crates-io]` replaces a *source*, so `omega-rs` is the entry
    // even though the dependency above it reads `omega`. Getting this wrong
    // yields a patch cargo silently ignores.
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
    // `cargo install --path ~/dev/omega` is a release build of a durable
    // checkout and should link; `cargo install --git` unpacks into
    // `$CARGO_HOME` and cargo may delete it afterwards. The profile does not
    // tell those apart, and for a while this code asked it.
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
        SourceTree::detect().is_some(),
        "the tests run from a checkout somebody keeps"
    );
}

#[test]
fn a_path_that_is_not_a_checkout_says_so() {
    let error = SourceTree::at("/etc").unwrap_err().to_string();

    // A patch pointing at nothing would fail three commands later, naming a
    // file nobody wrote.
    assert!(error.contains("not an omega checkout"), "{error}");
}

#[test]
fn the_line_omega_new_prints_names_only_things_the_plugin_has() {
    let hint = Scaffold::placement_hint(&unit());
    let lib = Scaffold::new().unit_lib(&unit()).unwrap();

    // The hint is pasted into a Rust file and has to compile there, so every
    // name in it has to exist in the plugin it names. These are the two
    // halves, and they are edited a file apart.
    assert!(hint.contains("battery_widget::UNIT"), "{hint}");
    assert!(lib.contains("pub const UNIT"), "{lib}");
    assert!(hint.contains("battery_widget::Settings"), "{hint}");
    assert!(lib.contains("pub struct Settings"), "{lib}");
    assert!(hint.contains("low: 15"), "{hint}");
    assert!(lib.contains("pub low: u8"), "{lib}");

    // Fully qualified: a hint that needs a second hint about an import is a
    // hint that failed.
    assert!(hint.contains("omega_document::Modules::widget"), "{hint}");
}
