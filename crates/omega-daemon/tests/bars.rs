use std::sync::Arc;

use omega_daemon::hub::Hub;
use omega_daemon::manifest::ManifestStore;
use omega_daemon::plugins::{PluginRegistry, Request};
use omega_daemon::reconcile::{Action, PresentationProvider};
use omega_document::{Bars, Document, Modules};
use omega_proto::omega::{StateDocument, SurfaceKind, ViewTree, invoke, module, result};
use omega_proto::{Manifest, PluginName, Surface, SurfaceId};

struct Fixture {
    hub: Hub,
    plugins: PluginRegistry,
    provider: PresentationProvider,
}

impl Fixture {
    fn new() -> Self {
        let hub = Hub::new();
        let plugins = PluginRegistry::detached(hub.clone());
        let manifest = Manifest::new(&Self::name(), "0.1.0").exposing([
            Surface::new(
                &"indicator".parse::<SurfaceId>().unwrap(),
                SurfaceKind::Widget,
            ),
            Surface::new(
                &"details".parse::<SurfaceId>().unwrap(),
                SurfaceKind::Widget,
            ),
        ]);
        plugins.adopt(&ManifestStore::from_manifests([manifest.clone()]));
        let provider = PresentationProvider::new(
            plugins.clone(),
            Arc::new(ManifestStore::from_manifests([manifest])),
        );
        Self {
            hub,
            plugins,
            provider,
        }
    }

    fn plan(
        &self,
        document: &StateDocument,
    ) -> Result<
        Vec<omega_daemon::reconcile::presentations::InstanceChange>,
        omega_daemon::reconcile::ProviderError,
    > {
        let desired = self.provider.prepare(document)?;
        Ok(PresentationProvider::plan(
            &desired,
            &self.plugins.installed_presentations(),
        ))
    }

    fn name() -> PluginName {
        "wifi".parse::<PluginName>().unwrap()
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
    let _guard = fixture.plugins.connected(&Fixture::name(), requests);
    let responder = tokio::spawn(Fixture::answer(inbox, 8));
    let mut document = Fixture::document();
    let plan = fixture.plan(&document).unwrap();
    assert_eq!(plan.len(), 2);
    fixture.provider.apply(&plan).await.unwrap();
    assert!(fixture.plan(&document).unwrap().is_empty());
    assert_eq!(fixture.hub.view_snapshot().len(), 2);

    let module::Kind::Widget(widget) = document.bars[0].modules[0].kind.as_mut().unwrap() else {
        panic!()
    };
    widget
        .config
        .insert("expanded".into(), omega_proto::IntoValue::into_value(true));
    let plan = fixture.plan(&document).unwrap();
    assert_eq!(plan.len(), 2);
    assert!(plan.iter().all(|change| change.action == Action::Update));
    fixture.provider.apply(&plan).await.unwrap();

    let (_, mut views) = fixture.hub.subscribe_views();
    let plan = fixture.plan(&StateDocument::default()).unwrap();
    fixture.provider.apply(&plan).await.unwrap();
    assert!(fixture.plugins.instances().is_empty());
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
    assert_eq!(surfaces, ["indicator", "details"]);
    assert!(
        operations[6..]
            .iter()
            .all(|op| matches!(op, invoke::Op::RemoveWidget(_)))
    );
}

#[tokio::test]
async fn disconnected_instances_remain_pending() {
    let fixture = Fixture::new();
    let plan = fixture.plan(&Fixture::document()).unwrap();
    assert!(fixture.provider.apply(&plan).await.is_err());
    assert_eq!(fixture.plan(&Fixture::document()).unwrap().len(), 2);
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
    assert!(fixture.plan(&document).is_err());
}

#[tokio::test]
async fn a_missing_anchor_is_retried_and_captured_facts_survive_disconnect() {
    let fixture = Fixture::new();
    let (requests, inbox) = tokio::sync::mpsc::channel(8);
    let guard = fixture.plugins.connected(&Fixture::name(), requests);
    let responder = tokio::spawn(Fixture::answer(inbox, 2));
    let changes = fixture.plan(&Fixture::document()).unwrap();
    let popup = changes
        .iter()
        .find(|change| change.anchor.is_some())
        .unwrap();
    assert!(
        fixture
            .provider
            .apply(std::slice::from_ref(popup))
            .await
            .is_err()
    );
    assert!(fixture.plugins.installed_presentations().is_empty());

    let changes = fixture.plan(&Fixture::document()).unwrap();
    fixture.provider.apply(&changes).await.unwrap();
    responder.await.unwrap();
    let captured = fixture.plugins.installed_presentations();
    assert_eq!(captured.len(), 2);
    let popup = changes
        .iter()
        .find(|change| change.anchor.is_some())
        .unwrap();
    assert_eq!(captured[&popup.address].anchor, popup.anchor);
    let desired = fixture.provider.prepare(&Fixture::document()).unwrap();
    assert!(PresentationProvider::plan(&desired, &captured).is_empty());
    drop(guard);
    assert!(fixture.plugins.installed_presentations().is_empty());
    assert!(PresentationProvider::plan(&desired, &captured).is_empty());
    let retry = PresentationProvider::plan(&desired, &fixture.plugins.installed_presentations());
    assert_eq!(retry.len(), 2);
    assert!(retry.iter().all(|change| change.action == Action::Create));
}

#[tokio::test]
async fn an_old_render_cannot_install_into_a_new_session() {
    let fixture = Fixture::new();
    let (requests, mut inbox) = tokio::sync::mpsc::channel(1);
    let old = fixture.plugins.connected(&Fixture::name(), requests);
    let change = fixture.plan(&Fixture::document()).unwrap().remove(0);
    let plugins = fixture.plugins.clone();
    let configure = tokio::spawn(async move {
        plugins
            .configure_instance(&change.address, change.config)
            .await
    });
    let request = inbox.recv().await.unwrap();
    let _new = fixture
        .plugins
        .connected(&Fixture::name(), tokio::sync::mpsc::channel(1).0);
    drop(old);
    request
        .answer
        .send(Ok(result::Outcome::View(ViewTree::default())))
        .unwrap();
    assert!(configure.await.unwrap().is_err());
    assert!(fixture.plugins.instances().is_empty());
    assert!(fixture.hub.view_snapshot().is_empty());
}

#[test]
fn unsupported_modules_are_refused_instead_of_omitted() {
    let fixture = Fixture::new();
    let mut document = Fixture::document();
    document.bars[0].modules[0].kind = Some(module::Kind::Clock(Default::default()));
    assert!(fixture.plan(&document).is_err());
}
