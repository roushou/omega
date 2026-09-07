//! Bar composition: a surface, instantiated as many times as the document
//! says, each instance its own view.

mod common;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use common::{Harness, widget_manifest};
use omega_core::UnitName;
use omega_daemon::hub::{Hub, SurfaceRef, ViewUpdate};
use omega_daemon::manifest::ManifestStore;
use omega_daemon::reconcile::{Action, BarProvider, Provider};
use omega_daemon::units::UnitTable;
use omega_document::{Bars, Document, Modules};
use omega_wire::omega::{StateDocument, ViewNode, ViewTree};

fn surface(id: &str) -> omega_core::SurfaceId {
    omega_core::SurfaceId::parse(id).unwrap()
}

fn module(id: &str) -> omega_core::ModuleId {
    omega_core::ModuleId::parse(id).unwrap()
}

fn unit(name: &str) -> UnitName {
    UnitName::parse(name).unwrap()
}

fn view() -> ViewTree {
    ViewTree {
        root: Some(ViewNode {
            key: "clock".into(),
            r#type: "text".into(),
            props: HashMap::new(),
            children: Vec::new(),
        }),
        revision: 0,
    }
}

/// A bar with the same widget unit in it twice, which is the case the module
/// dimension exists for.
fn two_clocks() -> StateDocument {
    Document::new()
        .bar(Bars::top(
            "main",
            vec![
                Modules::plain_widget("clock-left", "clock-widget"),
                Modules::plain_widget("clock-right", "clock-widget"),
            ],
        ))
        .into_inner()
}

fn provider(hub: Hub, units: UnitTable) -> BarProvider {
    BarProvider::new(
        hub,
        units,
        Arc::new(ManifestStore::from_manifests([widget_manifest(
            "clock-widget",
            "clock",
        )])),
    )
}

#[test]
fn each_declared_instance_is_planned_separately() {
    let hub = Hub::new();
    let provider = provider(hub.clone(), UnitTable::detached(Hub::new()));

    let plan = provider.plan(&two_clocks());

    assert_eq!(plan.len(), 2, "one clock unit, two instances");
    assert_eq!(plan[0].target, "clock-left");
    assert_eq!(plan[1].target, "clock-right");
    assert!(plan.iter().all(|change| change.action == Action::Create));
}

#[test]
fn an_instance_that_has_a_view_is_already_converged() {
    let hub = Hub::new();
    hub.publish_view(ViewUpdate {
        surface: SurfaceRef::module(unit("clock-widget"), surface("clock"), module("clock-left")),
        view: view(),
    });

    let plan = provider(hub, UnitTable::detached(Hub::new())).plan(&two_clocks());

    assert_eq!(plan.len(), 1);
    assert_eq!(plan[0].target, "clock-right");
}

#[tokio::test]
async fn an_instance_no_bar_declares_is_dropped() {
    let hub = Hub::new();
    hub.publish_view(ViewUpdate {
        surface: SurfaceRef::module(unit("clock-widget"), surface("clock"), module("clock-gone")),
        view: view(),
    });

    let provider = provider(hub.clone(), UnitTable::detached(Hub::new()));
    let document = Document::new().into_inner();

    let plan = provider.plan(&document);
    assert_eq!(plan[0].action, Action::Delete);
    assert_eq!(plan[0].target, "clock-gone");

    provider.apply(&document, &plan).await.unwrap();
    assert!(
        hub.view_snapshot().is_empty(),
        "the shell should stop being told about it"
    );
}

#[tokio::test]
async fn a_unit_that_is_not_connected_does_not_fail_the_convergence() {
    let hub = Hub::new();
    let provider = provider(hub.clone(), UnitTable::detached(Hub::new()));
    let document = two_clocks();

    // Nothing is connected: rendering cannot happen, and that is a warning
    // rather than a failed convergence — the unit may still be starting.
    let plan = provider.plan(&document);
    provider.apply(&document, &plan).await.unwrap();
    assert!(hub.view_snapshot().is_empty());
}

#[tokio::test]
async fn a_unit_renders_every_instance_the_document_gives_it() {
    let manifest = widget_manifest("clock-widget", "clock");
    let harness = Harness::new(
        "bars-render",
        ManifestStore::from_manifests([manifest.clone()]),
    );
    let token = harness.register_unit("clock-widget");

    let mut transport = harness.connect(&manifest.hash(), token.as_str()).await;
    transport.recv().await.unwrap().unwrap(); // Welcome

    // The daemon asks; a real unit would answer through the SDK.
    let sessions = harness.units.clone();
    let asked = tokio::spawn(async move {
        let provider = BarProvider::new(
            harness.hub.clone(),
            sessions,
            Arc::new(ManifestStore::from_manifests([widget_manifest(
                "clock-widget",
                "clock",
            )])),
        );
        let document = two_clocks();
        let plan = provider.plan(&document);
        provider.apply(&document, &plan).await.unwrap();
        provider
    });

    // Answer both RenderWidget requests the way the SDK does.
    let mut answered = Vec::new();
    while answered.len() < 2 {
        let frame = tokio::time::timeout(Duration::from_secs(2), transport.recv())
            .await
            .expect("the daemon should ask")
            .unwrap()
            .unwrap();

        let Some(omega_wire::omega::frame::Body::Invoke(invoke)) = frame.body else {
            continue;
        };
        let Some(omega_wire::omega::invoke::Op::RenderWidget(render)) = invoke.op else {
            continue;
        };

        // Every request names the instance being rendered, and the daemon
        // allocates even stream ids for its own requests.
        assert_eq!(render.surface_id, "clock");
        assert!(frame.stream_id % 2 == 0, "{}", frame.stream_id);
        answered.push(render.module_id.clone());

        transport
            .send(omega_wire::omega::Frame {
                stream_id: frame.stream_id,
                body: Some(omega_wire::omega::frame::Body::Result(
                    omega_wire::omega::Result {
                        outcome: Some(omega_wire::omega::result::Outcome::View(view())),
                        done: true,
                    },
                )),
            })
            .await
            .unwrap();
    }

    let provider = asked.await.unwrap();
    answered.sort();
    assert_eq!(answered, vec!["clock-left", "clock-right"]);

    // Both instances now exist as separate views of one surface.
    assert!(provider.plan(&two_clocks()).is_empty());
}
