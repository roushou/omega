mod common;

use std::collections::HashMap;
use std::time::Duration;

use common::{TempSocket, unit_name};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use omega_daemon::hub::{Hub, SurfaceRef, ViewUpdate};
use omega_daemon::shell::ShellServer;
use omega_proto::ModuleId;
use omega_proto::Observation;
use omega_proto::omega::{Value, ViewNode, ViewTree, value};

fn surface(id: &str) -> omega_proto::SurfaceId {
    omega_proto::SurfaceId::try_from(id).unwrap()
}

fn view_with_text(text: &str) -> ViewTree {
    let mut props = HashMap::new();
    props.insert(
        "text".to_string(),
        Value {
            kind: Some(value::Kind::StringValue(text.into())),
        },
    );
    ViewTree {
        root: Some(ViewNode {
            key: "battery".into(),
            r#type: "box".into(),
            props: HashMap::new(),
            children: vec![ViewNode {
                key: "label".into(),
                r#type: "text".into(),
                props,
                children: Vec::new(),
                ..Default::default()
            }],
            ..Default::default()
        }),
        revision: 0,
        ..Default::default()
    }
}

/// The battery widget's surface, as the daemon would qualify it.
fn battery(text: &str) -> ViewUpdate {
    {
        let surface = SurfaceRef::new(unit_name("battery-widget"), surface("battery"));
        ViewUpdate {
            instance: omega_proto::instance::InstanceKey {
                id: omega_proto::instance::InstanceId::try_from(format!(
                    "test-{}-{}-{}",
                    surface.unit,
                    surface.surface,
                    surface
                        .module
                        .as_ref()
                        .map(ToString::to_string)
                        .unwrap_or_default()
                ))
                .unwrap(),
                incarnation: "test-session"
                    .parse::<omega_proto::instance::IncarnationId>()
                    .unwrap(),
            },
            presentation: omega_proto::omega::Presentation {
                kind: Some(omega_proto::omega::presentation::Kind::Window(
                    omega_proto::omega::WindowPresentation {
                        title: "Test".into(),
                        app_id: "org.omega.example".into(),
                        width: 480,
                        height: 320,
                        min_width: 1,
                        min_height: 1,
                    },
                )),
            },
            requested: 2,
            observed: 2,
            destroyed: false,
            surface,
            view: view_with_text(text),
        }
    }
}

fn parse(line: &str) -> serde_json::Value {
    serde_json::from_str(line.trim()).unwrap()
}

/// The `<unit>.<surface>` a JSON line names.
fn surface_of(line: &str) -> String {
    let line = parse(line);
    format!(
        "{}.{}",
        line["unit"].as_str().unwrap(),
        line["surface"].as_str().unwrap()
    )
}

fn text_of(line: &str) -> String {
    parse(line)["view"]["root"]["children"][0]["props"]["text"]["stringValue"]
        .as_str()
        .unwrap()
        .to_string()
}

#[tokio::test]
async fn hub_owns_view_revisions_and_dedupes_unchanged() {
    let hub = Hub::new();
    let (snapshot, mut rx) = hub.subscribe_views();
    assert!(snapshot.is_empty());

    // First publish: revision 1, content preserved.
    let view_a = view_with_text("50%");
    hub.publish_view(battery("50%")).unwrap();
    let first = rx.recv().await.unwrap();
    assert_eq!(first.view.revision, 1);
    assert_eq!(first.view.root, view_a.root);

    // Identical re-publish: deduplicated — no new revision, no broadcast.
    hub.publish_view(battery("50%")).unwrap();
    assert!(
        rx.try_recv().is_err(),
        "identical view must not be rebroadcast"
    );

    // Changed content: revision 2.
    let view_b = view_with_text("87%");
    hub.publish_view(battery("87%")).unwrap();
    let second = rx.recv().await.unwrap();
    assert_eq!(second.view.revision, 2);
    assert_eq!(second.view.root, view_b.root);

    // The global sequence also advances across different surface addresses.
    hub.publish_view({
        let surface = SurfaceRef::new(unit_name("clock-widget"), surface("battery"));
        ViewUpdate {
            instance: omega_proto::instance::InstanceKey {
                id: omega_proto::instance::InstanceId::try_from(format!(
                    "test-{}-{}-{}",
                    surface.unit,
                    surface.surface,
                    surface
                        .module
                        .as_ref()
                        .map(ToString::to_string)
                        .unwrap_or_default()
                ))
                .unwrap(),
                incarnation: "test-session"
                    .parse::<omega_proto::instance::IncarnationId>()
                    .unwrap(),
            },
            presentation: omega_proto::omega::Presentation {
                kind: Some(omega_proto::omega::presentation::Kind::Window(
                    omega_proto::omega::WindowPresentation {
                        title: "Test".into(),
                        app_id: "org.omega.example".into(),
                        width: 480,
                        height: 320,
                        min_width: 1,
                        min_height: 1,
                    },
                )),
            },
            requested: 2,
            observed: 2,
            destroyed: false,
            surface,
            view: view_with_text("12:00"),
        }
    })
    .unwrap();
    let clock = rx.recv().await.unwrap();
    assert_eq!(clock.surface.unit, unit_name("clock-widget"));
    assert_eq!(clock.view.revision, 3);
}

#[tokio::test]
async fn shell_socket_streams_views() {
    let hub = Hub::new();
    let path = TempSocket::new("shell");
    let socket = path.socket();
    let server = ViewFixture::server(socket.clone(), hub.clone());
    tokio::spawn(async move {
        let _ = server.run().await;
    });

    // Published before connecting: delivered as the snapshot.
    hub.publish_view(battery("50%")).unwrap();

    let stream = socket.connect_stream().await.unwrap();
    let mut reader = BufReader::new(stream);

    let first = ViewFixture::attach(&mut reader).await;
    assert_eq!(surface_of(&first), "battery-widget.battery");
    assert_eq!(text_of(&first), "50%");

    // Published after connecting: delivered live.
    hub.publish_view(battery("87%")).unwrap();

    let mut second = String::new();
    tokio::time::timeout(Duration::from_secs(2), reader.read_line(&mut second))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(text_of(&second), "87%");
}

#[tokio::test]
async fn the_observation_socket_streams_state_as_it_changes() {
    let hub = Hub::new();
    let path = TempSocket::new("observer-state");
    let socket = path.socket();
    let server = ShellServer::bind_at(socket.clone(), hub.clone()).unwrap();
    tokio::spawn(async move {
        let _ = server.run().await;
    });

    hub.publish_state(battery_patch(0.42)).unwrap();

    let stream = socket.connect_stream().await.unwrap();
    let mut reader = BufReader::new(stream);

    // The snapshot first...
    let mut snapshot = String::new();
    reader.read_line(&mut snapshot).await.unwrap();
    assert_eq!(parse(&snapshot)["revision"], "1");

    // ...then every change, without being asked again. Ten times, because a
    // stream that works once and then stops is the failure worth catching.
    for revision in 2..=11 {
        hub.publish_state(battery_patch(revision as f64 / 100.0))
            .unwrap();

        let mut line = String::new();
        tokio::time::timeout(Duration::from_secs(2), reader.read_line(&mut line))
            .await
            .unwrap_or_else(|_| panic!("no line for revision {revision}"))
            .unwrap();

        assert_eq!(parse(&line)["topic"], "battery");
        assert_eq!(parse(&line)["revision"], revision.to_string());

        // Decode daemon output with the shared observation reader used by CLI clients.
        let topic = Observation::topic(&line).expect("a state line parses as a topic");
        assert_eq!(topic.topic, "battery");
        assert!(matches!(
            topic.value,
            Some(omega_proto::omega::state_topic::Value::Battery(_))
        ));
    }
}

#[tokio::test]
async fn a_view_line_is_not_mistaken_for_a_topic() {
    let line = serde_json::to_string(&battery("50%")).unwrap();
    assert!(Observation::topic(&line).is_none(), "{line}");
}

fn battery_patch(level: f64) -> omega_proto::omega::StatePatch {
    omega_proto::omega::StatePatch {
        topics: vec![omega_proto::omega::StateTopic {
            topic: "battery".into(),
            revision: 0,
            value: Some(omega_proto::omega::state_topic::Value::Battery(
                omega_proto::omega::BatteryState {
                    level,
                    charging: false,
                    seconds_to_empty: 0,
                    seconds_to_full: 0,
                },
            )),
        }],
    }
}

#[tokio::test]
async fn what_a_unit_was_showing_goes_when_the_unit_does() {
    let hub = Hub::new();
    let units = omega_daemon::units::UnitTable::detached(hub.clone());
    let unit = unit_name("battery-widget");

    // A unit connects, is asked for an instance, and shows it.
    let guard = units.connected(&unit, tokio::sync::mpsc::channel(1).0);
    hub.publish_view({
        let surface = SurfaceRef::module(unit.clone(), surface("battery"), module("top-bar-1"));
        ViewUpdate {
            instance: omega_proto::instance::InstanceKey {
                id: omega_proto::instance::InstanceId::try_from(format!(
                    "test-{}-{}-{}",
                    surface.unit,
                    surface.surface,
                    surface
                        .module
                        .as_ref()
                        .map(ToString::to_string)
                        .unwrap_or_default()
                ))
                .unwrap(),
                incarnation: "test-session"
                    .parse::<omega_proto::instance::IncarnationId>()
                    .unwrap(),
            },
            presentation: omega_proto::omega::Presentation {
                kind: Some(omega_proto::omega::presentation::Kind::Window(
                    omega_proto::omega::WindowPresentation {
                        title: "Test".into(),
                        app_id: "org.omega.example".into(),
                        width: 480,
                        height: 320,
                        min_width: 1,
                        min_height: 1,
                    },
                )),
            },
            requested: 2,
            observed: 2,
            destroyed: false,
            surface,
            view: view_with_text("80%"),
        }
    })
    .unwrap();
    assert_eq!(hub.view_snapshot().len(), 1);

    let (_snapshot, mut observed) = hub.subscribe_views();
    drop(guard);

    // Disconnect must clear published views and instance ownership.
    assert!(
        hub.view_snapshot().is_empty(),
        "a view outlived the unit that published it"
    );

    // Disconnection must clear observer views.
    let cleared = tokio::time::timeout(Duration::from_secs(2), observed.recv())
        .await
        .expect("observers are told")
        .unwrap();
    assert_eq!(cleared.surface.unit, unit);
    assert!(cleared.view.root.is_none(), "{cleared:?}");
}

fn module(id: &str) -> ModuleId {
    ModuleId::try_from(id).unwrap()
}

struct ViewFixture;
impl ViewFixture {
    fn server(socket: omega_proto::Socket, hub: Hub) -> ShellServer {
        let units = omega_daemon::units::UnitTable::detached(hub.clone());
        units.adopt(&omega_daemon::manifest::ManifestStore::from_manifests([
            common::widget_manifest("battery-widget", "battery"),
        ]));
        let stop = omega_daemon::Shutdown::new();
        ShellServer::bind_at(socket.clone(), hub.clone())
            .unwrap()
            .serving(
                omega_daemon::supervisor::Supervisor::new(socket, units.clone(), stop.clone()),
                units,
                omega_daemon::broker::Brokerage::new(hub, stop),
            )
    }
    async fn attach(reader: &mut BufReader<tokio::net::UnixStream>) -> String {
        let request = omega_proto::Observation::request(
            1,
            omega_proto::omega::invoke::Op::AttachRenderer(omega_proto::omega::AttachRenderer {
                build_fingerprint: String::new(),
                scope: Some(omega_proto::omega::attach_renderer::Scope::Unit(
                    "battery-widget".into(),
                )),
                features: vec![1, 2, 3, 4, 5, 6, 7, 8],
            }),
        );
        reader
            .get_mut()
            .write_all((Observation::line(&request).unwrap() + "\n").as_bytes())
            .await
            .unwrap();
        loop {
            let mut line = String::new();
            tokio::time::timeout(Duration::from_secs(2), reader.read_line(&mut line))
                .await
                .unwrap()
                .unwrap();
            let json: serde_json::Value = serde_json::from_str(&line).unwrap();
            if json.get("view").is_some() {
                return line;
            }
            if json.get("result").is_some() {
                assert!(json["result"]["error"].is_null(), "{json}");
                continue;
            }
        }
    }
}
