mod common;
use omega_daemon::{
    Shutdown,
    broker::Brokerage,
    hub::Hub,
    manifest::ManifestStore,
    plugins::{PluginRegistry, SessionGuard},
    shell::ShellServer,
    supervisor::Supervisor,
};
use omega_proto::{
    IntoValue, Observation, Socket,
    omega::{self, frame, invoke, presentation, result},
};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

struct Fixture {
    plugins: PluginRegistry,
    hub: Hub,
    socket: Socket,
    guard: Option<SessionGuard>,
    server: tokio::task::JoinHandle<()>,
    _path: common::TempSocket,
}
impl Fixture {
    fn new(tag: &str) -> Self {
        let path = common::TempSocket::new(tag);
        let socket = path.socket();
        let hub = Hub::new();
        let plugins = PluginRegistry::detached(hub.clone());
        let manifest =
            common::widget_manifest("example", "panel").serving([omega::CommandEndpoint {
                id: "activate".into(),
            }]);
        plugins.adopt(&ManifestStore::from_manifests([manifest]));
        let guard = Self::connect_plugin(&plugins);
        let shutdown = Shutdown::new();
        let server = ShellServer::bind_at(socket.clone(), hub.clone())
            .unwrap()
            .serving(
                Supervisor::new(socket.clone(), plugins.clone(), shutdown.clone()),
                plugins.clone(),
                Brokerage::new(hub.clone(), shutdown),
            );
        let server = tokio::spawn(async move {
            server.run().await.unwrap();
        });
        Self {
            plugins,
            hub,
            socket,
            guard: Some(guard),
            server,
            _path: path,
        }
    }
    fn connect_plugin(plugins: &PluginRegistry) -> SessionGuard {
        Self::connect_plugin_answering(plugins, None)
    }
    fn connect_plugin_answering(
        plugins: &PluginRegistry,
        command_answer: Option<result::Outcome>,
    ) -> SessionGuard {
        let (tx, mut rx) = tokio::sync::mpsc::channel::<omega_daemon::plugins::Request>(16);
        tokio::spawn(async move {
            while let Some(request) = rx.recv().await {
                let answer = match request.op {
                    invoke::Op::RenderWidget(render) => result::Outcome::View(Self::view(
                        render
                            .config
                            .get("label")
                            .cloned()
                            .unwrap_or_else(|| "default".into_value()),
                    )),
                    invoke::Op::SurfaceEvent(event) => {
                        assert_eq!(event.binding, 991);
                        result::Outcome::Value(event.value.unwrap())
                    }
                    invoke::Op::SurfaceLifecycle(_) => result::Outcome::Ok(Default::default()),
                    invoke::Op::RemoveWidget(_) => result::Outcome::Ok(Default::default()),
                    invoke::Op::CallCommand(call) => {
                        assert_eq!(call.command, "activate");
                        command_answer
                            .clone()
                            .unwrap_or_else(|| result::Outcome::Value(call.args[0].clone()))
                    }
                    other => panic!("unexpected plugin request {other:?}"),
                };
                let _ = request.answer.send(Ok(answer));
            }
        });
        plugins.connected(&common::plugin_name("example"), tx)
    }
    fn view(label: omega::Value) -> omega::ViewTree {
        omega::ViewTree {
            revision: 0,
            root: Some(omega::ViewNode {
                key: "button".into(),
                r#type: "button".into(),
                props: [("label".into(), label.clone())].into(),
                events: [(
                    "press".into(),
                    omega::Bind {
                        local: 0,
                        command: "activate".into(),
                        args: vec![label],
                    },
                )]
                .into(),
                ..Default::default()
            }),
            ..Default::default()
        }
    }
    fn request(singleton: &str, label: &str) -> omega::CreateInstance {
        omega::CreateInstance {
            plugin: "example".into(),
            surface: "panel".into(),
            singleton: singleton.into(),
            config: [("label".into(), label.into_value())].into(),
            presentation: Some(omega::Presentation {
                kind: Some(presentation::Kind::Window(omega::WindowPresentation {
                    title: "Example".into(),
                    app_id: "org.omega.example".into(),
                    width: 480,
                    height: 320,
                    min_width: 1,
                    min_height: 1,
                })),
            }),
        }
    }
    async fn observer(&self) -> Observer {
        Observer {
            reader: BufReader::new(self.socket.connect_stream().await.unwrap()),
            stream: 1,
        }
    }
    fn attachment(features: Vec<i32>) -> invoke::Op {
        invoke::Op::AttachRenderer(omega::AttachRenderer {
            build_fingerprint: String::new(),
            scope: Some(omega::attach_renderer::Scope::Plugin("example".into())),
            features,
        })
    }
    fn snapshot(answer: result::Outcome) -> omega::InstanceSnapshot {
        let result::Outcome::Instances(mut list) = answer else {
            panic!("expected instances, got {answer:?}")
        };
        assert_eq!(list.instances.len(), 1);
        list.instances.remove(0)
    }
    fn refusal(answer: result::Outcome, code: omega::ErrorCode) {
        let result::Outcome::Error(error) = answer else {
            panic!("expected refusal, got {answer:?}")
        };
        assert_eq!(error.code, code as i32, "{}", error.message);
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.server.abort();
    }
}
struct Observer {
    reader: BufReader<tokio::net::UnixStream>,
    stream: u64,
}
impl Observer {
    async fn ask(&mut self, op: invoke::Op) -> result::Outcome {
        let stream = self.stream;
        self.stream += 2;
        self.reader
            .get_mut()
            .write_all(
                (Observation::line(&Observation::request(stream, op)).unwrap() + "\n").as_bytes(),
            )
            .await
            .unwrap();
        loop {
            let mut line = String::new();
            let size =
                tokio::time::timeout(Duration::from_secs(2), self.reader.read_line(&mut line))
                    .await
                    .unwrap()
                    .unwrap();
            assert_ne!(size, 0);
            if let Some(frame) = Observation::answer(&line) {
                assert_eq!(frame.stream_id, stream);
                let Some(frame::Body::Result(answer)) = frame.body else {
                    panic!("expected result")
                };
                return answer.outcome.unwrap();
            }
        }
    }
    async fn create(&mut self, singleton: &str, label: &str) -> omega::InstanceSnapshot {
        Fixture::snapshot(
            self.ask(invoke::Op::CreateInstance(Fixture::request(
                singleton, label,
            )))
            .await,
        )
    }
}

#[tokio::test]
async fn independent_settings_singleton_reuse_and_dismissal_share_one_registry() {
    let fixture = Fixture::new("instances-lifecycle");
    let mut owner = fixture.observer().await;
    let first = owner.create("main", "one").await;
    let second = owner.create("", "two").await;
    assert_ne!(first.instance, second.instance);
    assert_eq!(
        first.view.as_ref().unwrap().root.as_ref().unwrap().props["label"],
        "one".into_value()
    );
    assert_eq!(
        second.view.as_ref().unwrap().root.as_ref().unwrap().props["label"],
        "two".into_value()
    );
    assert_eq!(owner.create("main", "one").await.instance, first.instance);
    Fixture::refusal(
        owner
            .ask(invoke::Op::CreateInstance(Fixture::request(
                "main", "changed",
            )))
            .await,
        omega::ErrorCode::FailedPrecondition,
    );
    let mut renderer = fixture.observer().await;
    let result::Outcome::Instances(attached) = renderer
        .ask(Fixture::attachment(vec![1, 2, 5, 6, 7, 8]))
        .await
    else {
        panic!("attach failed")
    };
    assert_eq!(attached.instances.len(), 2);
    renderer
        .ask(invoke::Op::ReportPresentation(omega::ReportPresentation {
            instance: first.instance.clone(),
            observed: omega::PresentationState::Closed as i32,
        }))
        .await;
    let closed = fixture
        .plugins
        .inspect_instances(None)
        .instances
        .into_iter()
        .find(|instance| instance.instance == first.instance)
        .unwrap();
    assert_eq!(closed.requested, omega::PresentationState::Closed as i32);
    assert_eq!(closed.observed, omega::PresentationState::Closed as i32);
    drop(renderer);
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            let snapshot = fixture
                .plugins
                .inspect_instances(None)
                .instances
                .into_iter()
                .find(|instance| instance.instance == first.instance)
                .unwrap();
            assert_eq!(snapshot.requested, omega::PresentationState::Closed as i32);
            if snapshot.observed == omega::PresentationState::Unspecified as i32 {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("disconnect must clear observation without reopening");
    let mut recovered = fixture.observer().await;
    let result::Outcome::Instances(attached) = recovered
        .ask(Fixture::attachment(vec![1, 2, 5, 6, 7, 8]))
        .await
    else {
        panic!("reattach failed")
    };
    assert_eq!(
        attached
            .instances
            .iter()
            .find(|instance| instance.instance == first.instance)
            .unwrap()
            .requested,
        omega::PresentationState::Closed as i32
    );
    let reopened = owner.create("main", "one").await;
    assert_eq!(reopened.instance, first.instance);
    assert_eq!(reopened.requested, omega::PresentationState::Visible as i32);
    assert_eq!(
        reopened.observed,
        omega::PresentationState::Unspecified as i32
    );
}

#[tokio::test]
async fn renderer_interactions_resolve_retained_bindings_and_reject_stale_revisions() {
    let fixture = Fixture::new("instances-input");
    let mut owner = fixture.observer().await;
    let first = owner.create("", "bound argument").await;
    let mut renderer = fixture.observer().await;
    renderer.ask(Fixture::attachment(vec![1, 2, 5, 7, 8])).await;
    Fixture::refusal(
        renderer
            .ask(invoke::Op::GetDeployment(Default::default()))
            .await,
        omega::ErrorCode::PermissionDenied,
    );
    Fixture::refusal(
        renderer.ask(invoke::Op::Act(Default::default())).await,
        omega::ErrorCode::PermissionDenied,
    );
    let event = omega::Interact {
        instance: first.instance.clone(),
        revision: first.view.as_ref().unwrap().revision,
        node: "button".into(),
        event: "press".into(),
        value: None,
    };
    assert_eq!(
        renderer.ask(invoke::Op::Interact(event.clone())).await,
        result::Outcome::Value("bound argument".into_value())
    );
    fixture
        .plugins
        .publish_instance(
            &common::plugin_name("example"),
            &omega::PublishView {
                instance: first.instance.clone(),
                surface_id: "panel".into(),
                view: Some(Fixture::view("new argument".into_value())),
            },
        )
        .unwrap();
    Fixture::refusal(
        renderer.ask(invoke::Op::Interact(event)).await,
        omega::ErrorCode::FailedPrecondition,
    );
}

#[tokio::test]
async fn retained_interactions_share_eligibility_but_keep_command_authorization() {
    let fixture = Fixture::new("instances-eligibility");
    let mut owner = fixture.observer().await;
    let first = owner.create("", "argument").await;
    let mut renderer = fixture.observer().await;
    renderer.ask(Fixture::attachment(vec![1, 2, 5, 7, 8])).await;
    for case in ["disabled", "busy", "duplicate", "mixed", "undeclared"] {
        let mut target = Fixture::view("argument".into_value()).root.unwrap();
        match case {
            "mixed" => target.events.get_mut("press").unwrap().local = 991,
            "undeclared" => target.events.get_mut("press").unwrap().command = "missing".into(),
            _ => {}
        }
        let children = if case == "duplicate" {
            vec![target.clone(), target]
        } else {
            vec![target]
        };
        let mut parent = omega::ViewNode {
            r#type: "stack".into(),
            key: "parent".into(),
            children,
            ..Default::default()
        };
        if matches!(case, "disabled" | "busy") {
            parent.props.insert(case.into(), true.into_value());
        }
        fixture
            .plugins
            .publish_instance(
                &common::plugin_name("example"),
                &omega::PublishView {
                    instance: first.instance.clone(),
                    surface_id: "panel".into(),
                    view: Some(omega::ViewTree {
                        root: Some(parent),
                        revision: 0,
                        ..Default::default()
                    }),
                },
            )
            .unwrap();
        let current = fixture.plugins.inspect_instances(None).instances.remove(0);
        let code = match case {
            "disabled" | "busy" => omega::ErrorCode::FailedPrecondition,
            "undeclared" => omega::ErrorCode::PermissionDenied,
            _ => omega::ErrorCode::InvalidArgument,
        };
        Fixture::refusal(
            renderer
                .ask(invoke::Op::Interact(omega::Interact {
                    instance: current.instance,
                    revision: current.view.unwrap().revision,
                    node: "button".into(),
                    event: "press".into(),
                    value: None,
                }))
                .await,
            code,
        );
    }
}

#[tokio::test]
async fn renderer_command_answers_preserve_refusals_and_reject_unexpected_outcomes() {
    for (outcome, code) in [
        (
            result::Outcome::View(Default::default()),
            omega::ErrorCode::InvalidArgument,
        ),
        (
            result::Outcome::Error(omega::Error {
                code: omega::ErrorCode::PermissionDenied as i32,
                message: "command denied".into(),
            }),
            omega::ErrorCode::PermissionDenied,
        ),
    ] {
        let mut fixture = Fixture::new("instances-command-answer");
        drop(fixture.guard.take());
        fixture.guard = Some(Fixture::connect_plugin_answering(
            &fixture.plugins,
            Some(outcome),
        ));
        let mut owner = fixture.observer().await;
        let first = owner.create("", "argument").await;
        let mut renderer = fixture.observer().await;
        renderer.ask(Fixture::attachment(vec![1, 2, 5, 7, 8])).await;
        Fixture::refusal(
            renderer
                .ask(invoke::Op::Interact(omega::Interact {
                    instance: first.instance,
                    revision: first.view.unwrap().revision,
                    node: "button".into(),
                    event: "press".into(),
                    value: None,
                }))
                .await,
            code,
        );
    }
}

#[tokio::test]
async fn restart_invalidates_instances_and_destroy_is_explicitly_non_idempotent() {
    let mut fixture = Fixture::new("instances-restart");
    let mut owner = fixture.observer().await;
    let first = owner.create("main", "one").await;
    drop(fixture.guard.take());
    assert!(fixture.hub.view_snapshot().is_empty());
    fixture.guard = Some(Fixture::connect_plugin(&fixture.plugins));
    let new = owner.create("main", "one").await;
    assert_ne!(first.instance, new.instance);
    let destroy = |instance| {
        invoke::Op::ChangePresentation(omega::ChangePresentation {
            instance,
            action: omega::PresentationAction::Destroy as i32,
        })
    };
    Fixture::refusal(
        owner.ask(destroy(first.instance)).await,
        omega::ErrorCode::FailedPrecondition,
    );
    assert!(matches!(
        owner.ask(destroy(new.instance.clone())).await,
        result::Outcome::Ok(_)
    ));
    Fixture::refusal(
        owner.ask(destroy(new.instance)).await,
        omega::ErrorCode::FailedPrecondition,
    );
    assert!(fixture.plugins.inspect_instances(None).instances.is_empty());
}

#[tokio::test]
async fn attachment_negotiates_features_and_never_grants_an_unknown_scope() {
    let fixture = Fixture::new("instances-features");
    let mut owner = fixture.observer().await;
    owner.create("", "one").await;
    Fixture::refusal(
        owner.ask(Fixture::attachment(vec![1])).await,
        omega::ErrorCode::FailedPrecondition,
    );
    Fixture::refusal(
        owner.ask(Fixture::attachment(vec![1, 2, 3, 7, 8])).await,
        omega::ErrorCode::Unimplemented,
    );
    Fixture::refusal(
        owner
            .ask(invoke::Op::AttachRenderer(omega::AttachRenderer {
                build_fingerprint: String::new(),
                scope: Some(omega::attach_renderer::Scope::Plugin("unknown".into())),
                features: vec![1, 2, 5, 7, 8],
            }))
            .await,
        omega::ErrorCode::InvalidArgument,
    );
    assert!(matches!(
        owner.ask(Fixture::attachment(vec![1, 2, 5, 7, 8])).await,
        result::Outcome::Instances(_)
    ));
    Fixture::refusal(
        owner.ask(Fixture::attachment(vec![1, 2, 5, 7, 8])).await,
        omega::ErrorCode::PermissionDenied,
    );
}

#[tokio::test]
async fn configured_windows_do_not_reopen_during_reconciliation() {
    let fixture = Fixture::new("instances-configured");
    let manifest = fixture
        .plugins
        .manifest(&common::plugin_name("example"))
        .unwrap()
        .manifest;
    let provider = omega_daemon::reconcile::PresentationProvider::new(
        fixture.plugins.clone(),
        std::sync::Arc::new(ManifestStore::from_manifests([manifest])),
    );
    let plan = |document: &omega::StateDocument| {
        provider.prepare(document).map(|desired| {
            omega_daemon::reconcile::PresentationProvider::plan(
                &desired,
                &fixture.plugins.installed_presentations(),
            )
        })
    };
    let create = Fixture::request("", "configured");
    let document = omega::StateDocument {
        presentations: vec![omega::ConfiguredPresentation {
            id: "window".into(),
            plugin: create.plugin,
            surface: create.surface,
            config: create.config,
            presentation: create.presentation,
        }],
        ..Default::default()
    };
    provider.apply(&plan(&document).unwrap()).await.unwrap();
    let snapshot = fixture.plugins.inspect_instances(None).instances.remove(0);
    let mut owner = fixture.observer().await;
    owner
        .ask(invoke::Op::ChangePresentation(omega::ChangePresentation {
            instance: snapshot.instance,
            action: omega::PresentationAction::Close as i32,
        }))
        .await;
    assert!(plan(&document).unwrap().is_empty());
    let transient = owner.create("", "transient").await;
    provider
        .apply(&plan(&omega::StateDocument::default()).unwrap())
        .await
        .unwrap();
    assert_eq!(
        fixture.plugins.inspect_instances(None).instances[0].instance,
        transient.instance
    );
}

#[tokio::test]
async fn replacing_an_attachment_revokes_the_previous_renderer() {
    let fixture = Fixture::new("instances-renderer-lease");
    let mut owner = fixture.observer().await;
    let snapshot = owner.create("", "one").await;
    let mut old = fixture.observer().await;
    old.ask(Fixture::attachment(vec![1, 2, 5, 7, 8])).await;
    let mut new = fixture.observer().await;
    new.ask(Fixture::attachment(vec![1, 2, 5, 7, 8])).await;
    let report = invoke::Op::ReportPresentation(omega::ReportPresentation {
        instance: snapshot.instance,
        observed: omega::PresentationState::Visible as i32,
    });
    Fixture::refusal(
        old.ask(report.clone()).await,
        omega::ErrorCode::PermissionDenied,
    );
    assert!(matches!(new.ask(report).await, result::Outcome::Ok(_)));
}

#[tokio::test]
async fn local_events_are_resolved_from_the_retained_tree_not_public_commands() {
    let fixture = Fixture::new("local-surface-event");
    let mut owner = fixture.observer().await;
    let first = owner.create("", "local").await;
    let mut tree = Fixture::view("local".into_value());
    tree.root.as_mut().unwrap().events.insert(
        "press".into(),
        omega::Bind {
            local: 991,
            ..Default::default()
        },
    );
    fixture
        .plugins
        .publish_instance(
            &common::plugin_name("example"),
            &omega::PublishView {
                instance: first.instance.clone(),
                surface_id: "panel".into(),
                view: Some(tree.clone()),
            },
        )
        .unwrap();
    let snapshot = fixture.plugins.inspect_instances(None).instances.remove(0);
    let mut renderer = fixture.observer().await;
    renderer.ask(Fixture::attachment(vec![1, 2, 5, 7, 8])).await;
    let event = omega::Interact {
        instance: first.instance.clone(),
        revision: snapshot.view.unwrap().revision,
        node: "button".into(),
        event: "press".into(),
        value: Some("input".into_value()),
    };
    assert_eq!(
        renderer.ask(invoke::Op::Interact(event.clone())).await,
        result::Outcome::Value("input".into_value())
    );
    tree.root
        .as_mut()
        .unwrap()
        .events
        .get_mut("press")
        .unwrap()
        .command = "activate".into();
    fixture
        .plugins
        .publish_instance(
            &common::plugin_name("example"),
            &omega::PublishView {
                instance: first.instance,
                surface_id: "panel".into(),
                view: Some(tree),
            },
        )
        .unwrap();
    let revision = fixture
        .plugins
        .inspect_instances(None)
        .instances
        .remove(0)
        .view
        .unwrap()
        .revision;
    Fixture::refusal(
        renderer
            .ask(invoke::Op::Interact(omega::Interact { revision, ..event }))
            .await,
        omega::ErrorCode::InvalidArgument,
    );
}

#[tokio::test]
async fn a_plugin_can_dismiss_only_its_own_current_instance() {
    use omega_daemon::session::{
        admission::Peer, dispatch::Dispatcher, subscriptions::Subscriptions,
    };
    let fixture = Fixture::new("plugin-dismiss");
    let instance = fixture
        .plugins
        .create_instance(&Fixture::request("", "owned"))
        .await
        .unwrap();
    let shutdown = Shutdown::new();
    let dispatcher = Dispatcher::new(
        fixture.hub.clone(),
        Supervisor::new(
            fixture.socket.clone(),
            fixture.plugins.clone(),
            shutdown.clone(),
        ),
        fixture.plugins.clone(),
        Brokerage::new(fixture.hub.clone(), shutdown),
    );
    let name = common::plugin_name("example");
    let manifest = common::widget_manifest("example", "panel");
    let peer = Peer::plugin(name.clone(), &manifest).unwrap();
    let mut subscriptions = Subscriptions::of(&name, &manifest);
    let request = omega::Invoke {
        op: Some(invoke::Op::ChangePresentation(omega::ChangePresentation {
            instance: instance.instance.clone(),
            action: omega::PresentationAction::Close as i32,
        })),
    };
    let stranger = Peer::plugin(common::plugin_name("stranger"), &manifest).unwrap();
    assert!(
        dispatcher
            .invoke(&stranger, &mut subscriptions, &request)
            .await
            .is_err()
    );
    dispatcher
        .invoke(&peer, &mut subscriptions, &request)
        .await
        .unwrap();
    for action in [
        omega::PresentationAction::Present,
        omega::PresentationAction::Destroy,
    ] {
        let request = omega::Invoke {
            op: Some(invoke::Op::ChangePresentation(omega::ChangePresentation {
                instance: instance.instance.clone(),
                action: action as i32,
            })),
        };
        assert!(
            dispatcher
                .invoke(&peer, &mut subscriptions, &request)
                .await
                .is_err()
        );
    }
    let mut expired = instance.instance.unwrap();
    expired.incarnation = "expired".into();
    let request = omega::Invoke {
        op: Some(invoke::Op::ChangePresentation(omega::ChangePresentation {
            instance: Some(expired),
            action: omega::PresentationAction::Close as i32,
        })),
    };
    assert!(
        dispatcher
            .invoke(&peer, &mut subscriptions, &request)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn deployment_reports_only_live_renderer_builds_and_replacement_revokes_the_old_one() {
    let fixture = Fixture::new("renderer-builds");
    let mut operator = fixture.observer().await;
    let invoke::Op::AttachRenderer(mut request) = Fixture::attachment(vec![1, 2, 5, 6, 7, 8])
    else {
        unreachable!()
    };
    request.build_fingerprint = "a".repeat(64);
    let mut first = fixture.observer().await;
    assert!(matches!(
        first.ask(invoke::Op::AttachRenderer(request.clone())).await,
        result::Outcome::Instances(_)
    ));
    let result::Outcome::Deployment(status) = operator
        .ask(invoke::Op::GetDeployment(Default::default()))
        .await
    else {
        panic!("deployment")
    };
    assert_eq!(status.renderers, vec![request.clone()]);

    request.build_fingerprint = "b".repeat(64);
    let mut second = fixture.observer().await;
    assert!(matches!(
        second
            .ask(invoke::Op::AttachRenderer(request.clone()))
            .await,
        result::Outcome::Instances(_)
    ));
    drop(first);
    let result::Outcome::Deployment(status) = operator
        .ask(invoke::Op::GetDeployment(Default::default()))
        .await
    else {
        panic!("deployment")
    };
    assert_eq!(status.renderers, vec![request]);
    drop(second);
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            let result::Outcome::Deployment(status) = operator
                .ask(invoke::Op::GetDeployment(Default::default()))
                .await
            else {
                panic!("deployment")
            };
            if status.renderers.is_empty() {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn invalid_renderer_fingerprints_are_refused_but_legacy_attachments_remain_supported() {
    let fixture = Fixture::new("renderer-invalid-build");
    let invoke::Op::AttachRenderer(mut request) = Fixture::attachment(vec![1, 2, 5, 6, 7, 8])
    else {
        unreachable!()
    };
    request.build_fingerprint = "not-a-build".into();
    let mut renderer = fixture.observer().await;
    Fixture::refusal(
        renderer
            .ask(invoke::Op::AttachRenderer(request.clone()))
            .await,
        omega::ErrorCode::InvalidArgument,
    );
    request.build_fingerprint.clear();
    assert!(matches!(
        renderer.ask(invoke::Op::AttachRenderer(request)).await,
        result::Outcome::Instances(_)
    ));
}

#[tokio::test]
async fn required_placements_are_reported_even_without_a_renderer() {
    let fixture = Fixture::new("renderer-required-placements");
    let address = omega_daemon::hub::SurfaceRef::module(
        common::plugin_name("example"),
        "panel".parse::<omega_proto::SurfaceId>().unwrap(),
        "slot".parse::<omega_proto::ModuleId>().unwrap(),
    );
    fixture
        .plugins
        .configure_instance(&address, Default::default())
        .await
        .unwrap();
    let mut operator = fixture.observer().await;
    operator.create("window", "standalone").await;
    let result::Outcome::Deployment(status) = operator
        .ask(invoke::Op::GetDeployment(Default::default()))
        .await
    else {
        panic!("deployment")
    };
    assert!(status.renderers.is_empty());
    assert_eq!(
        status.renderer_placements,
        vec![omega::PlacementAttachment {
            plugin: "example".into(),
            surface: "panel".into(),
            placement: "slot".into(),
        }]
    );
    fixture.plugins.remove_instance(&address).await.unwrap();
    let result::Outcome::Deployment(status) = operator
        .ask(invoke::Op::GetDeployment(Default::default()))
        .await
    else {
        panic!("deployment")
    };
    assert!(status.renderer_placements.is_empty());
}

#[tokio::test]
async fn deployment_reports_readiness_without_exposing_views_and_expires_session_facts() {
    use omega::PluginReadiness;
    use omega::RenderReadiness;

    let mut fixture = Fixture::new("health");
    let mut observer = fixture.observer().await;
    let snapshot = observer.create("health", "private-view-content").await;
    let result::Outcome::Deployment(status) = observer
        .ask(invoke::Op::GetDeployment(Default::default()))
        .await
    else {
        panic!("expected deployment snapshot");
    };
    assert_eq!(status.plugin_health[0].readiness(), PluginReadiness::Ready);
    assert_eq!(
        status.plugin_health[0].instances[0].instance,
        snapshot.instance
    );
    assert!(
        !serde_json::to_string(&status)
            .unwrap()
            .contains("private-view-content")
    );

    fixture
        .plugins
        .publish_instance(
            &common::plugin_name("example"),
            &omega::PublishView {
                surface_id: "panel".into(),
                instance: snapshot.instance.clone(),
                view: Some(omega::ViewTree {
                    readiness: RenderReadiness::Waiting as i32,
                    pending_topics: vec!["network".into()],
                    ..Default::default()
                }),
            },
        )
        .unwrap();
    let result::Outcome::Deployment(status) = observer
        .ask(invoke::Op::GetDeployment(Default::default()))
        .await
    else {
        panic!("expected deployment snapshot");
    };
    assert_eq!(
        status.plugin_health[0].readiness(),
        PluginReadiness::Waiting
    );
    assert_eq!(
        status.plugin_health[0].instances[0].pending_topics,
        ["network"]
    );

    let stale = snapshot.instance;
    drop(fixture.guard.take());
    let result::Outcome::Deployment(status) = observer
        .ask(invoke::Op::GetDeployment(Default::default()))
        .await
    else {
        panic!("expected deployment snapshot");
    };
    assert!(status.plugin_health[0].instances.is_empty());
    assert!(
        fixture
            .plugins
            .publish_instance(
                &common::plugin_name("example"),
                &omega::PublishView {
                    surface_id: "panel".into(),
                    instance: stale,
                    view: Some(omega::ViewTree {
                        readiness: RenderReadiness::Ready as i32,
                        ..Default::default()
                    }),
                }
            )
            .is_err()
    );
}
