//! Convergence: the document is intent, and a plan is what the machine would
//! have to do about it.

mod common;

use common::{TempDir, widget_manifest};
use omega_core::UnitName;
use omega_daemon::Shutdown;
use omega_daemon::hub::Hub;
use omega_daemon::manifest::ManifestStore;
use omega_daemon::reconcile::{Action, EnvironmentProvider, Provider, Reconciler, UnitProvider};
use omega_daemon::supervisor::Supervisor;
use omega_daemon::units::UnitTable;
use omega_document::{Document, Units};
use omega_wire::Socket;

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
        &tmp.layout(),
        [unit("battery-widget")],
    );

    // Putting a crate in the workspace is already a declaration; the document
    // exists to override that, not to repeat it.
    let plan = provider.plan(&Document::new().into_inner());

    assert_eq!(plan.len(), 1);
    assert_eq!(plan[0].action, Action::Create);
    assert_eq!(plan[0].target, "battery-widget");
}

#[test]
fn a_document_can_turn_one_unit_off() {
    let tmp = TempDir::new("disable");
    let provider = UnitProvider::new(
        supervisor("disable"),
        &tmp.layout(),
        [unit("battery-widget"), unit("clock")],
    );

    let document = Document::new()
        .unit(Units::disabled("battery-widget"))
        .into_inner();
    let plan = provider.plan(&document);

    // The blast radius of the change is the unit it names.
    assert_eq!(plan.len(), 1);
    assert_eq!(plan[0].target, "clock");
    assert_eq!(plan[0].action, Action::Create);
}

#[test]
fn a_document_naming_an_unbuilt_unit_is_reported() {
    let tmp = TempDir::new("unknown");
    let provider = UnitProvider::new(
        supervisor("unknown"),
        &tmp.layout(),
        [unit("battery-widget")],
    );

    let document = Document::new()
        .unit(Units::enabled("does-not-exist"))
        .into_inner();
    let plan = provider.plan(&document);

    let reported = plan
        .iter()
        .find(|change| change.target == "does-not-exist")
        .expect("an unbuildable reference must not vanish silently");
    assert!(reported.summary.contains("does not contain"), "{reported}");
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

    let plan = provider.plan(&document);
    assert_eq!(plan.len(), 2);
    assert!(plan.iter().all(|change| change.action == Action::Create));

    provider.apply(&document, &plan).await.unwrap();
    let written = std::fs::read_to_string(provider.path()).unwrap();
    assert_eq!(written, "EDITOR=hx\nOMEGA_HOST=laptop\n");

    // Converged: nothing left to do.
    assert!(provider.plan(&document).is_empty());

    // Removing a declaration removes the variable.
    let smaller = Document::new().env("EDITOR", "hx").into_inner();
    let plan = provider.plan(&smaller);
    assert_eq!(plan[0].action, Action::Delete);
    assert_eq!(plan[0].target, "OMEGA_HOST");

    provider.apply(&smaller, &plan).await.unwrap();
    assert_eq!(
        std::fs::read_to_string(provider.path()).unwrap(),
        "EDITOR=hx\n"
    );
}

#[tokio::test]
async fn a_machine_that_matches_its_document_plans_nothing() {
    let tmp = TempDir::new("converged");
    let layout = tmp.layout();
    std::fs::create_dir_all(&layout.state).unwrap();

    let reconciler = Reconciler::new()
        .with(UnitProvider::new(
            supervisor("converged"),
            &layout,
            [unit("battery-widget")],
        ))
        .with(EnvironmentProvider::new(&layout));

    // Nothing built is enabled, nothing declared: the empty document over an
    // empty machine is already converged.
    let document = Document::new()
        .unit(Units::disabled("battery-widget"))
        .into_inner();

    assert!(reconciler.plan(&document).is_empty());
    assert!(reconciler.converge(&document).await.is_empty());
}
