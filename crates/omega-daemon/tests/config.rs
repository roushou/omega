//! What a unit was configured with.
//!
//! A unit's settings are construction, not state: a plugin's fields are built
//! out of them. So they reach a unit exactly once, in its `Welcome`, and the
//! reconciler's job is to make sure what is on file when it connects is what
//! the document says — and to run it again when that changes.

mod common;

use common::{Harness, TempDir, widget_manifest};
use omega_core::UnitName;
use omega_daemon::Shutdown;
use omega_daemon::hub::Hub;
use omega_daemon::manifest::ManifestStore;
use omega_daemon::reconcile::{Action, ConfigProvider, Provider};
use omega_daemon::supervisor::Supervisor;
use omega_daemon::units::{UnitControl, UnitTable};
use omega_document::{Document, Units};
use omega_wire::omega::frame;
use omega_wire::{Socket, Values};

fn unit(name: &str) -> UnitName {
    UnitName::parse(name).unwrap()
}

/// A table that has adopted one built unit, and a supervisor over it.
fn machine(tag: &str) -> (UnitTable, Supervisor) {
    let units = UnitTable::detached(Hub::new());
    units.adopt(&ManifestStore::from_manifests([widget_manifest(
        "battery-widget",
        "battery",
    )]));
    let supervisor = Supervisor::new(
        Socket::at(format!("/tmp/omega-config-{tag}.sock")),
        units.clone(),
        Shutdown::new(),
    );
    (units, supervisor)
}

fn warning(low: u8) -> Values {
    Values::new().with("low-threshold", low)
}

#[tokio::test]
async fn a_unit_is_configured_before_anything_runs() {
    let _tmp = TempDir::new("configure");
    let (units, supervisor) = machine("configure");
    let provider = ConfigProvider::new(units.clone(), supervisor);

    let document = Document::new()
        .unit(Units::configured("battery-widget", &warning(20)))
        .into_inner();

    let plan = provider.plan(&document);
    assert_eq!(plan.len(), 1);
    assert_eq!(plan[0].action, Action::Update);
    assert_eq!(plan[0].target, "battery-widget");

    provider.apply(&document, &plan).await.unwrap();
    assert_eq!(
        Values::from_map(units.config(&unit("battery-widget"))).get::<u8>("low-threshold"),
        Some(20)
    );

    // Converged: nothing left to do.
    assert!(provider.plan(&document).is_empty());
}

#[tokio::test]
async fn taking_a_setting_out_of_the_document_is_a_change_too() {
    let _tmp = TempDir::new("unconfigure");
    let (units, supervisor) = machine("unconfigure");
    let provider = ConfigProvider::new(units.clone(), supervisor);

    let configured = Document::new()
        .unit(Units::configured("battery-widget", &warning(20)))
        .into_inner();
    provider
        .apply(&configured, &provider.plan(&configured))
        .await
        .unwrap();

    // A unit whose settings were removed has changed exactly as much as one
    // whose settings were edited: it has to go back to its defaults, and the
    // only way it can is by being built again.
    let bare = Document::new().into_inner();
    let plan = provider.plan(&bare);
    assert_eq!(plan.len(), 1);
    assert_eq!(plan[0].target, "battery-widget");

    provider.apply(&bare, &plan).await.unwrap();
    assert!(units.config(&unit("battery-widget")).is_empty());
}

#[test]
fn a_unit_nobody_configured_needs_nothing_done() {
    let _tmp = TempDir::new("quiet");
    let (units, supervisor) = machine("quiet");
    let provider = ConfigProvider::new(units, supervisor);

    // The common case, and the one that must not churn: a document that says
    // nothing about settings matches a machine that was told none.
    assert!(provider.plan(&Document::new().into_inner()).is_empty());
}

#[test]
fn a_unit_this_build_does_not_contain_is_not_configured() {
    let _tmp = TempDir::new("unbuilt");
    let (units, supervisor) = machine("unbuilt");
    let provider = ConfigProvider::new(units.clone(), supervisor);

    // Reported by the provider that knows what was built; inventing a record
    // here would leave a unit in the table that nothing can ever run.
    let document = Document::new()
        .unit(Units::configured("does-not-exist", &warning(20)))
        .into_inner();

    assert!(provider.plan(&document).is_empty());
    assert!(units.config(&unit("does-not-exist")).is_empty());
}

#[tokio::test]
async fn a_running_unit_is_run_again_when_its_settings_change() {
    let _tmp = TempDir::new("recycle");
    let (units, supervisor) = machine("recycle");
    let provider = ConfigProvider::new(units.clone(), supervisor);

    // Standing in for a supervised process: the count a restart increments.
    let cycle = tokio::sync::watch::channel(0u64).0;
    units.supervise(
        &unit("battery-widget"),
        UnitControl {
            stop: Shutdown::new(),
            cycle: cycle.clone(),
        },
    );

    let document = Document::new()
        .unit(Units::configured("battery-widget", &warning(20)))
        .into_inner();
    provider
        .apply(&document, &provider.plan(&document))
        .await
        .unwrap();

    // Settings arrive at construction, so a process already built without
    // them cannot be handed them — it has to be built again.
    assert_eq!(*cycle.borrow(), 1);

    // And a document that changed nothing cycles nothing: a converged machine
    // that restarts its units every pass is worse than one that never does.
    provider
        .apply(&document, &provider.plan(&document))
        .await
        .unwrap();
    assert_eq!(*cycle.borrow(), 1);
}

#[tokio::test]
async fn a_unit_is_told_its_settings_when_it_connects() {
    let harness = Harness::new(
        "welcome-config",
        ManifestStore::from_manifests([widget_manifest("test-unit", "battery")]),
    );
    harness.units.configure(
        &unit("test-unit"),
        warning(20).with("label", "batt").into_map(),
    );

    let token = harness.register_unit("test-unit");
    let hash = widget_manifest("test-unit", "battery").hash();
    let mut transport = harness.connect(&hash, token.as_str()).await;

    let welcome = match transport.recv().await.unwrap().unwrap().body {
        Some(frame::Body::Welcome(welcome)) => welcome,
        other => panic!("expected Welcome, got {other:?}"),
    };

    let settings = Values::from_map(welcome.config);
    assert_eq!(settings.get::<u8>("low-threshold"), Some(20));
    assert_eq!(settings.get::<String>("label").as_deref(), Some("batt"));
}
