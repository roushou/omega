mod common;

use std::collections::HashMap;
use std::time::Duration;

use common::{TempSocket, unit_name};
use tokio::io::{AsyncBufReadExt, BufReader};

use omega_daemon::hub::{Hub, SurfaceRef, ViewUpdate};
use omega_daemon::shell::ShellServer;
use omega_proto::ModuleId;
use omega_proto::Observation;
use omega_proto::omega::{Value, ViewNode, ViewTree, value};

fn surface(id: &str) -> omega_proto::SurfaceId {
    omega_proto::SurfaceId::parse(id).unwrap()
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
    }
}

/// The battery widget's surface, as the daemon would qualify it.
fn battery(text: &str) -> ViewUpdate {
    ViewUpdate {
        surface: SurfaceRef::new(unit_name("battery-widget"), surface("battery")),
        view: view_with_text(text),
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
    hub.publish_view(battery("50%"));
    let first = rx.recv().await.unwrap();
    assert_eq!(first.view.revision, 1);
    assert_eq!(first.view.root, view_a.root);

    // Identical re-publish: deduplicated — no new revision, no broadcast.
    hub.publish_view(battery("50%"));
    assert!(
        rx.try_recv().is_err(),
        "identical view must not be rebroadcast"
    );

    // Changed content: revision 2.
    let view_b = view_with_text("87%");
    hub.publish_view(battery("87%"));
    let second = rx.recv().await.unwrap();
    assert_eq!(second.view.revision, 2);
    assert_eq!(second.view.root, view_b.root);

    // Another unit's surface has its own independent sequence — even when it
    // chose the same surface id.
    hub.publish_view(ViewUpdate {
        surface: SurfaceRef::new(unit_name("clock-widget"), surface("battery")),
        view: view_with_text("12:00"),
    });
    let clock = rx.recv().await.unwrap();
    assert_eq!(clock.surface.unit, unit_name("clock-widget"));
    assert_eq!(clock.view.revision, 1);
}

#[tokio::test]
async fn shell_socket_streams_views() {
    let hub = Hub::new();
    let path = TempSocket::new("shell");
    let socket = path.socket();
    let server = ShellServer::bind_at(socket.clone(), hub.clone()).unwrap();
    tokio::spawn(async move {
        let _ = server.run().await;
    });

    // Published before connecting: delivered as the snapshot.
    hub.publish_view(battery("50%"));

    let stream = socket.connect_stream().await.unwrap();
    let mut reader = BufReader::new(stream);

    let mut first = String::new();
    reader.read_line(&mut first).await.unwrap();
    assert_eq!(surface_of(&first), "battery-widget.battery");
    assert_eq!(text_of(&first), "50%");

    // Published after connecting: delivered live.
    hub.publish_view(battery("87%"));

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

    hub.publish_state(battery_patch(0.42));

    let stream = socket.connect_stream().await.unwrap();
    let mut reader = BufReader::new(stream);

    // The snapshot first...
    let mut snapshot = String::new();
    reader.read_line(&mut snapshot).await.unwrap();
    assert_eq!(parse(&snapshot)["revision"], "1");

    // ...then every change, without being asked again. Ten times, because a
    // stream that works once and then stops is the failure worth catching.
    for revision in 2..=11 {
        hub.publish_state(battery_patch(revision as f64 / 100.0));

        let mut line = String::new();
        tokio::time::timeout(Duration::from_secs(2), reader.read_line(&mut line))
            .await
            .unwrap_or_else(|_| panic!("no line for revision {revision}"))
            .unwrap();

        assert_eq!(parse(&line)["topic"], "battery");
        assert_eq!(parse(&line)["revision"], revision.to_string());

        // The reader's half of the same contract: what this daemon writes,
        // `Observation::topic` reads back as a typed topic. `omega status`
        // depends on it and does not link the daemon to find out.
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
    let hub = Hub::new();
    let path = TempSocket::new("observer-views");
    let socket = path.socket();
    let server = ShellServer::bind_at(socket.clone(), hub.clone()).unwrap();
    tokio::spawn(async move {
        let _ = server.run().await;
    });

    hub.publish_view(battery("50%"));

    let stream = socket.connect_stream().await.unwrap();
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line).await.unwrap();

    // Both halves share one stream, so a reader that wanted topics has to be
    // able to tell it did not get one.
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
    hub.publish_view(ViewUpdate {
        surface: SurfaceRef::module(unit.clone(), surface("battery"), module("top-bar-1")),
        view: view_with_text("80%"),
    });
    assert_eq!(hub.view_snapshot().len(), 1);

    let (_snapshot, mut observed) = hub.subscribe_views();
    drop(guard);

    // Its process ended, so it is showing nothing. Keeping the last tree
    // would tell the reconciler this unit still knows about the instance the
    // document gave it — knowledge that lived in the process — and the unit
    // that comes back would never be told again.
    assert!(
        hub.view_snapshot().is_empty(),
        "a view outlived the unit that published it"
    );

    // And observers are told, so a shell can stop drawing rather than freeze
    // on a number nobody is taking.
    let cleared = tokio::time::timeout(Duration::from_secs(2), observed.recv())
        .await
        .expect("observers are told")
        .unwrap();
    assert_eq!(cleared.surface.unit, unit);
    assert!(cleared.view.root.is_none(), "{cleared:?}");
}

fn module(id: &str) -> ModuleId {
    ModuleId::parse(id).unwrap()
}
