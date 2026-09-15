//! Convergence: the document is intent, and a plan is what the machine would
//! have to do about it.

mod common;

use common::TempDir;
use omega_daemon::reconcile::units::UnitChange;
use omega_daemon::reconcile::{EnvironmentProvider, UnitProvider};
use omega_document::{Document, Units};
use omega_proto::UnitName;
use std::collections::BTreeSet;

#[test]
fn a_built_unit_the_document_never_mentions_still_runs() {
    let built = BTreeSet::from([UnitName::parse("battery-widget").unwrap()]);

    // Putting a crate in the workspace is already a declaration; the document
    // exists to override that, not to repeat it.
    let plan = UnitProvider::plan(&Document::new().into_inner(), &built, &BTreeSet::new()).unwrap();

    assert_eq!(plan.len(), 1);
    assert!(matches!(plan[0], UnitChange::Start(_)));
    assert_eq!(plan[0].name().as_str(), "battery-widget");
}

#[test]
fn a_document_can_turn_one_unit_off() {
    let built = BTreeSet::from([
        UnitName::parse("battery-widget").unwrap(),
        UnitName::parse("clock").unwrap(),
    ]);

    let document = Document::new()
        .unit(Units::disabled("battery-widget"))
        .into_inner();
    let plan = UnitProvider::plan(&document, &built, &BTreeSet::new()).unwrap();

    // The blast radius of the change is the unit it names.
    assert_eq!(plan.len(), 1);
    assert_eq!(plan[0].name().as_str(), "clock");
    assert!(matches!(plan[0], UnitChange::Start(_)));
}

#[test]
fn a_document_naming_an_unbuilt_unit_is_reported() {
    let built = BTreeSet::from([UnitName::parse("battery-widget").unwrap()]);

    let document = Document::new()
        .unit(Units::enabled("does-not-exist"))
        .into_inner();
    let error = UnitProvider::plan(&document, &built, &BTreeSet::new()).unwrap_err();
    assert!(error.to_string().contains("does-not-exist"));
}

#[test]
fn unit_planning_preserves_held_units_and_orders_starts_and_stops() {
    let [added, disabled, held, removed] =
        ["added", "disabled", "held", "removed"].map(|name| UnitName::parse(name).unwrap());
    let built = BTreeSet::from([added.clone(), disabled.clone(), held.clone()]);
    let running = BTreeSet::from([disabled.clone(), held.clone(), removed.clone()]);
    let document = Document::new()
        .unit(Units::disabled("disabled"))
        .into_inner();
    assert_eq!(
        UnitProvider::plan(&document, &built, &running).unwrap(),
        vec![
            UnitChange::Start(added.clone()),
            UnitChange::Stop(disabled),
            UnitChange::Stop(removed),
        ]
    );
    assert!(
        UnitProvider::plan(&document, &built, &BTreeSet::from([added, held]))
            .unwrap()
            .is_empty()
    );
}

#[test]
fn duplicate_and_invalid_unit_declarations_fail_with_literal_inputs() {
    let built = BTreeSet::from([UnitName::parse("clock").unwrap()]);
    let mut document = Document::new().unit(Units::enabled("clock")).into_inner();
    document.units.push(document.units[0].clone());
    assert!(UnitProvider::plan(&document, &built, &BTreeSet::new()).is_err());
    document.units.truncate(1);
    document.units[0].name = "invalid/name".into();
    assert!(UnitProvider::plan(&document, &built, &BTreeSet::new()).is_err());
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

    let plan = EnvironmentProvider::plan(
        EnvironmentProvider::prepare(&document).unwrap(),
        provider.installed().unwrap().as_deref(),
    );
    assert!(plan.is_some());

    provider.apply(plan.as_ref().unwrap()).unwrap();
    let written = std::fs::read_to_string(provider.path()).unwrap();
    assert_eq!(written, "EDITOR=hx\nOMEGA_HOST=laptop\n");

    // Converged: nothing left to do.
    assert!(
        EnvironmentProvider::plan(
            EnvironmentProvider::prepare(&document).unwrap(),
            provider.installed().unwrap().as_deref(),
        )
        .is_none()
    );

    // Removing a declaration removes the variable.
    let smaller = Document::new().env("EDITOR", "hx").into_inner();
    let plan = EnvironmentProvider::plan(
        EnvironmentProvider::prepare(&smaller).unwrap(),
        provider.installed().unwrap().as_deref(),
    );
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
    let built = BTreeSet::from([UnitName::parse("battery-widget").unwrap()]);
    let environment = EnvironmentProvider::new(&tmp.layout());
    let document = Document::new()
        .unit(Units::disabled("battery-widget"))
        .into_inner();
    assert!(
        UnitProvider::plan(&document, &built, &BTreeSet::new())
            .unwrap()
            .is_empty()
    );
    assert!(
        EnvironmentProvider::plan(
            EnvironmentProvider::prepare(&document).unwrap(),
            environment.installed().unwrap().as_deref(),
        )
        .is_none()
    );
}

#[tokio::test]
async fn environment_values_are_literal_shell_data() {
    let tmp = TempDir::new("environment-literal");
    let provider = EnvironmentProvider::new(&tmp.layout());
    let value = "a 'quoted' value\n$(printf executed) $HOME `printf executed`";
    let document = Document::new().env("OMEGA_VALUE", value).into_inner();
    provider
        .apply(
            &EnvironmentProvider::plan(
                EnvironmentProvider::prepare(&document).unwrap(),
                provider.installed().unwrap().as_deref(),
            )
            .unwrap(),
        )
        .unwrap();
    assert!(
        EnvironmentProvider::plan(
            EnvironmentProvider::prepare(&document).unwrap(),
            provider.installed().unwrap().as_deref(),
        )
        .is_none()
    );
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
    let change = EnvironmentProvider::plan(
        EnvironmentProvider::prepare(&document).unwrap(),
        provider.installed().unwrap().as_deref(),
    )
    .unwrap();
    document.environment[0].value = "changed".into();
    provider.apply(&change).unwrap();
    assert_eq!(
        std::fs::read_to_string(provider.path()).unwrap(),
        "EDITOR=hx\n"
    );
    std::fs::remove_file(provider.path()).unwrap();
    std::fs::create_dir(provider.path()).unwrap();
    assert!(provider.installed().is_err());
}

#[test]
fn invalid_environment_is_rejected_during_planning() {
    let tmp = TempDir::new("environment-invalid");
    let provider = EnvironmentProvider::new(&tmp.layout());
    let document = Document::new().env("INVALID;KEY", "value").into_inner();
    assert!(EnvironmentProvider::prepare(&document).is_err());
    assert!(!provider.path().exists());
}
