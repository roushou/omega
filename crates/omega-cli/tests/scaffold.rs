//! What `omega init` generates, checked against the invariants that make a
//! generated workspace build.

use omega_cli::checkout::SourceTree;
use omega_cli::scaffold::{PluginName, Published, Scaffold};
use omega_host::Toml;
use omega_host::cargo::{Inherited, Manifest};
use omega_proto::UnitName;

fn unit() -> UnitName {
    UnitName::parse("battery-widget").unwrap()
}

#[test]
fn every_member_inherits_from_the_workspace_and_never_declares() {
    let scaffold = Scaffold::new();
    let workspace = scaffold.workspace_manifest().unwrap();
    let dependencies = workspace
        .workspace()
        .unwrap()
        .unwrap()
        .dependencies()
        .unwrap();
    let declared: Vec<&str> = dependencies.names().collect();

    for member in [
        scaffold.unit_crate_manifest(&unit()).unwrap(),
        scaffold.system_manifest().unwrap(),
    ] {
        for (name, dependency) in member.dependencies().unwrap().iter() {
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
fn a_unit_and_the_config_plane_depend_on_different_things() {
    let scaffold = Scaffold::new();
    let unit_manifest = scaffold.unit_crate_manifest(&unit()).unwrap();
    let system_manifest = scaffold.system_manifest().unwrap();
    let unit_dependencies = unit_manifest.dependencies().unwrap();
    let unit_crate: Vec<&str> = unit_dependencies.names().collect();
    let system_dependencies = system_manifest.dependencies().unwrap();
    let system: Vec<&str> = system_dependencies.names().collect();

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
    let manifest = Scaffold::new().workspace_manifest().unwrap();
    let workspace = manifest.workspace().unwrap().unwrap();

    assert_eq!(workspace.resolver().unwrap(), Some("3"));
    assert_eq!(workspace.edition().unwrap(), Some("2024"));
    // The config plane is a member alongside the units it configures.
    assert_eq!(workspace.members().unwrap(), vec!["system".to_string()]);
    assert!(
        manifest.package().unwrap().is_none(),
        "a workspace root is not a package"
    );

    let encoded = Toml::encode(&manifest).unwrap();
    assert_eq!(
        Toml::decode::<Manifest>(&encoded).unwrap().to_string(),
        manifest.to_string()
    );
}

#[test]
fn the_config_plane_is_scaffolded_as_its_own_crate() {
    let scaffold = Scaffold::new();
    let system = scaffold.system_manifest().unwrap();

    assert_eq!(system.package().unwrap().unwrap().name().unwrap(), "system");
    assert_eq!(
        system.package().unwrap().unwrap().edition().unwrap(),
        Some(Inherited::Workspace)
    );
    // The authoring vocabulary and nothing else: a config says what the
    // machine should be and never names the protocol.
    assert_eq!(
        system.dependencies().unwrap().names().collect::<Vec<_>>(),
        vec!["omega-document", "omega-omarchy"]
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
    let unit_crate = Scaffold::new().unit_crate_manifest(&unit()).unwrap();
    let package = unit_crate.package().unwrap().unwrap();

    // The build copies `target/release/<crate name>`, so the crate name and
    // the unit name must be the same string.
    assert_eq!(package.name().unwrap(), unit().as_str());
    assert_eq!(package.edition().unwrap(), Some(Inherited::Workspace));
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
fn a_config_names_the_published_crates_whoever_scaffolded_it() {
    let manifest = Scaffold::from_source(Published::at("0.4.2"))
        .workspace_manifest()
        .unwrap();
    let dependencies = manifest
        .workspace()
        .unwrap()
        .unwrap()
        .dependencies()
        .unwrap();

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
    assert_eq!(
        Toml::decode::<Manifest>(&encoded).unwrap().to_string(),
        manifest.to_string()
    );
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
fn a_path_that_is_not_a_checkout_says_so() {
    let error = SourceTree::at("/etc").unwrap_err().to_string();

    // Reject nonexistent checkout paths before writing Cargo patches.
    assert!(error.contains("not an omega checkout"), "{error}");
}
