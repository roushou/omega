use std::sync::Arc;

use omega_daemon::hub::Hub;
use omega_daemon::manifest::ManifestStore;
use omega_daemon::reconcile::{Action, BarProvider};
use omega_daemon::units::{Request, UnitTable};
use omega_document::{Bars, Document, Modules};
use omega_proto::omega::{StateDocument, SurfaceKind, ViewTree, invoke, module, result};
use omega_proto::{Manifest, Surface, SurfaceId, UnitName};

struct Fixture {
    hub: Hub,
    units: UnitTable,
    provider: BarProvider,
}

impl Fixture {
    fn new() -> Self {
        let hub = Hub::new();
        let units = UnitTable::detached(hub.clone());
        let manifest = Manifest::new(&Self::name(), "0.1.0").exposing([
            Surface::new(&SurfaceId::parse("indicator").unwrap(), SurfaceKind::Widget),
            Surface::new(&SurfaceId::parse("details").unwrap(), SurfaceKind::Widget),
        ]);
        let provider = BarProvider::new(
            units.clone(),
            Arc::new(ManifestStore::from_manifests([manifest])),
        );
        Self {
            hub,
            units,
            provider,
        }
    }

    fn name() -> UnitName {
        UnitName::parse("wifi").unwrap()
    }

    fn document() -> StateDocument {
        Document::new()
            .bar(Bars::top(
                "main",
                vec![Modules::panel(
                    Modules::surface(Modules::plain_widget("slot", "wifi"), "indicator"),
                    "details",
                )],
            ))
            .into_inner()
    }

    async fn answer(
        mut inbox: tokio::sync::mpsc::Receiver<Request>,
        count: usize,
    ) -> Vec<invoke::Op> {
        let mut observed = Vec::new();
        for _ in 0..count {
            let request = inbox.recv().await.unwrap();
            let outcome = match &request.op {
                invoke::Op::RenderWidget(_) => result::Outcome::View(ViewTree::default()),
                invoke::Op::RemoveWidget(_) => result::Outcome::Ok(Default::default()),
                other => panic!("unexpected {other:?}"),
            };
            observed.push(request.op);
            request.answer.send(Ok(outcome)).unwrap();
        }
        observed
    }
}

#[tokio::test]
async fn both_surfaces_are_configured_updated_and_removed() {
    let fixture = Fixture::new();
    let (requests, inbox) = tokio::sync::mpsc::channel(8);
    let _guard = fixture.units.connected(&Fixture::name(), requests);
    let responder = tokio::spawn(Fixture::answer(inbox, 6));
    let mut document = Fixture::document();
    let plan = fixture.provider.plan(&document).unwrap();
    assert_eq!(plan.len(), 2);
    fixture.provider.apply(&plan).await.unwrap();
    assert!(fixture.provider.plan(&document).unwrap().is_empty());
    assert_eq!(fixture.hub.view_snapshot().len(), 2);

    let module::Kind::Widget(widget) = document.bars[0].modules[0].kind.as_mut().unwrap() else {
        panic!()
    };
    widget
        .config
        .insert("expanded".into(), omega_proto::IntoValue::into_value(true));
    let plan = fixture.provider.plan(&document).unwrap();
    assert_eq!(plan.len(), 2);
    assert!(plan.iter().all(|change| change.action == Action::Update));
    fixture.provider.apply(&plan).await.unwrap();

    let (_, mut views) = fixture.hub.subscribe_views();
    let plan = fixture.provider.plan(&StateDocument::default()).unwrap();
    fixture.provider.apply(&plan).await.unwrap();
    assert!(fixture.units.instances().is_empty());
    assert!(fixture.hub.view_snapshot().is_empty());
    for _ in 0..2 {
        assert!(views.recv().await.unwrap().view.root.is_none());
    }
    let operations = responder.await.unwrap();
    let surfaces: Vec<_> = operations[..2]
        .iter()
        .map(|op| match op {
            invoke::Op::RenderWidget(render) => render.surface_id.as_str(),
            _ => panic!(),
        })
        .collect();
    assert_eq!(surfaces, ["details", "indicator"]);
    assert!(
        operations[4..]
            .iter()
            .all(|op| matches!(op, invoke::Op::RemoveWidget(_)))
    );
}

#[tokio::test]
async fn disconnected_instances_remain_pending() {
    let fixture = Fixture::new();
    let plan = fixture.provider.plan(&Fixture::document()).unwrap();
    assert!(fixture.provider.apply(&plan).await.is_err());
    assert_eq!(
        fixture.provider.plan(&Fixture::document()).unwrap().len(),
        2
    );
}

#[test]
fn ambiguous_surface_is_a_validation_error() {
    let fixture = Fixture::new();
    let document = Document::new()
        .bar(Bars::top(
            "main",
            vec![Modules::plain_widget("slot", "wifi")],
        ))
        .into_inner();
    assert!(fixture.provider.plan(&document).is_err());
}

#[tokio::test]
async fn an_old_render_cannot_install_into_a_new_session() {
    let fixture = Fixture::new();
    let (requests, mut inbox) = tokio::sync::mpsc::channel(1);
    let old = fixture.units.connected(&Fixture::name(), requests);
    let change = fixture
        .provider
        .plan(&Fixture::document())
        .unwrap()
        .remove(0);
    let units = fixture.units.clone();
    let configure = tokio::spawn(async move {
        units
            .configure_instance(&change.address, change.config)
            .await
    });
    let request = inbox.recv().await.unwrap();
    let _new = fixture
        .units
        .connected(&Fixture::name(), tokio::sync::mpsc::channel(1).0);
    drop(old);
    request
        .answer
        .send(Ok(result::Outcome::View(ViewTree::default())))
        .unwrap();
    assert!(configure.await.unwrap().is_err());
    assert!(fixture.units.instances().is_empty());
    assert!(fixture.hub.view_snapshot().is_empty());
}

#[test]
fn unsupported_modules_are_refused_instead_of_omitted() {
    let fixture = Fixture::new();
    let mut document = Fixture::document();
    document.bars[0].modules[0].kind = Some(module::Kind::Clock(Default::default()));
    assert!(fixture.provider.plan(&document).is_err());
}
