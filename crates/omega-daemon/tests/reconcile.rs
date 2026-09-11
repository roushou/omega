//! Convergence: the document is intent, and a plan is what the machine would
//! have to do about it.

mod common;

use common::{TempDir, widget_manifest};
use omega_daemon::Shutdown;
use omega_daemon::hub::Hub;
use omega_daemon::manifest::ManifestStore;
use omega_daemon::reconcile::units::UnitChange;
use omega_daemon::reconcile::{EnvironmentProvider, UnitProvider};
use omega_daemon::supervisor::Supervisor;
use omega_daemon::units::UnitTable;
use omega_document::{Document, Units};
use omega_proto::Socket;
use omega_proto::UnitName;

impl TempDir {
    fn generation(&self) -> omega_host::Generation {
        let store = omega_host::Generations::new(&self.layout());
        store.stage().unwrap().commit().unwrap();
        store.pin_current().unwrap().unwrap()
    }
}

/// A table holding the manifests a test declares, which is what a supervisor
/// now needs instead of a manifest store of its own.
fn table_with(manifests: ManifestStore) -> UnitTable {
    let units = UnitTable::detached(Hub::new());
    units.adopt(&manifests);
    units
}

fn unit(name: &str) -> UnitName {
    UnitName::parse(name).unwrap()
}

fn supervisor(tag: &str) -> Supervisor {
    Supervisor::new(
        Socket::at(format!("/tmp/omega-reconcile-{tag}.sock")),
        table_with(ManifestStore::from_manifests([widget_manifest(
            "battery-widget",
            "battery",
        )])),
        Shutdown::new(),
    )
}

#[test]
fn a_built_unit_the_document_never_mentions_still_runs() {
    let tmp = TempDir::new("default-on");
    let provider = UnitProvider::new(
        supervisor("default-on"),
        tmp.generation(),
        [unit("battery-widget")],
    );

    // Putting a crate in the workspace is already a declaration; the document
    // exists to override that, not to repeat it.
    let plan = provider.plan(&Document::new().into_inner()).unwrap();

    assert_eq!(plan.len(), 1);
    assert!(matches!(plan[0], UnitChange::Start(_)));
    assert_eq!(plan[0].name().as_str(), "battery-widget");
}

#[test]
fn a_document_can_turn_one_unit_off() {
    let tmp = TempDir::new("disable");
    let provider = UnitProvider::new(
        supervisor("disable"),
        tmp.generation(),
        [unit("battery-widget"), unit("clock")],
    );

    let document = Document::new()
        .unit(Units::disabled("battery-widget"))
        .into_inner();
    let plan = provider.plan(&document).unwrap();

    // The blast radius of the change is the unit it names.
    assert_eq!(plan.len(), 1);
    assert_eq!(plan[0].name().as_str(), "clock");
    assert!(matches!(plan[0], UnitChange::Start(_)));
}

#[test]
fn a_document_naming_an_unbuilt_unit_is_reported() {
    let tmp = TempDir::new("unknown");
    let provider = UnitProvider::new(
        supervisor("unknown"),
        tmp.generation(),
        [unit("battery-widget")],
    );

    let document = Document::new()
        .unit(Units::enabled("does-not-exist"))
        .into_inner();
    let error = provider.plan(&document).unwrap_err();
    assert!(error.to_string().contains("does-not-exist"));
}

#[tokio::test]
async fn the_environment_converges_to_the_document() {
    let tmp = TempDir::new("env");
    let layout = tmp.layout();
    std::fs::create_dir_all(&layout.state).unwrap();
    let provider = EnvironmentProvider::new(&layout);

    let document = Document::new()
        .env("OMEGA_HOST", "laptop")
        .env("EDITOR", "hx")
        .into_inner();

    let plan = provider.plan(&document).unwrap();
    assert!(plan.is_some());

    provider.apply(plan.as_ref().unwrap()).unwrap();
    let written = std::fs::read_to_string(provider.path()).unwrap();
    assert_eq!(written, "EDITOR=hx\nOMEGA_HOST=laptop\n");

    // Converged: nothing left to do.
    assert!(provider.plan(&document).unwrap().is_none());

    // Removing a declaration removes the variable.
    let smaller = Document::new().env("EDITOR", "hx").into_inner();
    let plan = provider.plan(&smaller).unwrap();
    assert!(plan.is_some());

    provider.apply(plan.as_ref().unwrap()).unwrap();
    assert_eq!(
        std::fs::read_to_string(provider.path()).unwrap(),
        "EDITOR=hx\n"
    );
}

#[test]
fn a_machine_that_matches_its_document_plans_nothing() {
    let tmp = TempDir::new("converged");
    let units = UnitProvider::new(
        supervisor("converged"),
        tmp.generation(),
        [unit("battery-widget")],
    );
    let environment = EnvironmentProvider::new(&tmp.layout());
    let document = Document::new()
        .unit(Units::disabled("battery-widget"))
        .into_inner();
    assert!(units.plan(&document).unwrap().is_empty());
    assert!(environment.plan(&document).unwrap().is_none());
}

#[tokio::test]
async fn environment_values_are_literal_shell_data() {
    let tmp = TempDir::new("environment-literal");
    let provider = EnvironmentProvider::new(&tmp.layout());
    let value = "a 'quoted' value\n$(printf executed) $HOME `printf executed`";
    let document = Document::new().env("OMEGA_VALUE", value).into_inner();
    provider
        .apply(&provider.plan(&document).unwrap().unwrap())
        .unwrap();
    assert!(provider.plan(&document).unwrap().is_none());
    let output = std::process::Command::new("sh")
        .args(["-c", ". \"$1\"; printf %s \"$OMEGA_VALUE\"", "test"])
        .arg(provider.path())
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(String::from_utf8(output.stdout).unwrap(), value);
}

#[test]
fn environment_plan_retains_validated_contents_and_reports_read_failures() {
    let tmp = TempDir::new("environment-plan");
    let provider = EnvironmentProvider::new(&tmp.layout());
    let mut document = Document::new().env("EDITOR", "hx").into_inner();
    let change = provider.plan(&document).unwrap().unwrap();
    document.environment[0].value = "changed".into();
    provider.apply(&change).unwrap();
    assert_eq!(
        std::fs::read_to_string(provider.path()).unwrap(),
        "EDITOR=hx\n"
    );
    std::fs::remove_file(provider.path()).unwrap();
    std::fs::create_dir(provider.path()).unwrap();
    assert!(provider.plan(&Document::new().into_inner()).is_err());
}

#[test]
fn invalid_environment_is_rejected_during_planning() {
    let tmp = TempDir::new("environment-invalid");
    let provider = EnvironmentProvider::new(&tmp.layout());
    let document = Document::new().env("INVALID;KEY", "value").into_inner();
    assert!(provider.plan(&document).is_err());
    assert!(!provider.path().exists());
}
