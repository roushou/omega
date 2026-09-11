use std::sync::Arc;
use std::time::Duration;

use crate::hub::{Hub, SurfaceRef, ViewUpdate};
use crate::session::Subscriptions;
use crate::shell::{ShellConnection, ShellError};
use omega_proto::omega::{StateTopic, ViewNode, ViewTree, state_topic};
use omega_proto::{IntoValue, Refusal, SurfaceId, UnitName};
use tokio::io::{AsyncBufReadExt, BufReader, DuplexStream};
use tokio::time::Instant;

struct Fixture {
    connection: ShellConnection<DuplexStream>,
    reader: BufReader<DuplexStream>,
}

impl Fixture {
    fn new(hub: &Hub) -> Self {
        let (server, client) = tokio::io::duplex(64);
        Self {
            connection: ShellConnection::new(
                server,
                hub.subscribe_views().1,
                hub.subscribe_state().1,
                hub.clone(),
                Err(Refusal::denied("test")),
                None,
            ),
            reader: BufReader::new(client),
        }
    }

    fn view(id: usize, payload: &str) -> ViewUpdate {
        ViewUpdate {
            surface: SurfaceRef::new(
                UnitName::parse("example").unwrap(),
                SurfaceId::parse(format!("panel{id}")).unwrap(),
            ),
            view: ViewTree {
                root: Some(ViewNode {
                    key: payload.into(),
                    ..Default::default()
                }),
                revision: 1,
            },
        }
    }
}

#[tokio::test(start_paused = true)]
async fn line_progress_cannot_extend_a_view_batch_and_completed_views_are_released() {
    let mut fixture = Fixture::new(&Hub::new());
    let views: Vec<_> = (0..3)
        .map(|id| Arc::new(Fixture::view(id, &"x".repeat(1000))))
        .collect();
    let held: Vec<_> = views.iter().map(Arc::downgrade).collect();
    let start = Instant::now();
    let mut lines = 0;
    let result = tokio::select! {
        result = fixture.connection.write_views(views) => result,
        _ = async {
            loop {
                tokio::time::sleep(Duration::from_secs(4)).await;
                let mut line = String::new();
                assert_ne!(fixture.reader.read_line(&mut line).await.unwrap(), 0);
                lines += 1;
                tokio::task::yield_now().await;
                assert!(held[0].upgrade().is_none(), "sent view retained behind a later write");
            }
        } => unreachable!(),
    };
    assert!(matches!(result, Err(ShellError::SnapshotTimeout)));
    assert_eq!(Instant::now() - start, Duration::from_secs(5));
    assert_eq!(lines, 1);
    assert_eq!(fixture.connection.sent_views.len(), 1);
    assert!(held.iter().all(|view| view.upgrade().is_none()));
}

#[tokio::test(start_paused = true)]
async fn line_progress_cannot_extend_a_state_batch() {
    let mut fixture = Fixture::new(&Hub::new());
    let topics: Vec<_> = (0..3)
        .map(|id| StateTopic {
            topic: format!("unit.example.value{id}"),
            revision: 1,
            value: Some(state_topic::Value::Generic("x".repeat(1000).into_value())),
        })
        .collect();
    let start = Instant::now();
    let mut lines = 0;
    let subscriptions = Subscriptions::watcher();
    let result = tokio::select! {
        result = fixture.connection.write_topics(&subscriptions, &topics) => result,
        _ = async {
            loop {
                tokio::time::sleep(Duration::from_secs(2)).await;
                let mut line = String::new();
                assert_ne!(fixture.reader.read_line(&mut line).await.unwrap(), 0);
                let value: serde_json::Value = serde_json::from_str(&line).unwrap();
                assert_eq!(value["topic"], format!("unit.example.value{lines}"));
                lines += 1;
            }
        } => unreachable!(),
    };
    assert!(matches!(result, Err(ShellError::SnapshotTimeout)));
    assert_eq!(Instant::now() - start, Duration::from_secs(5));
    assert_eq!(lines, 2);
}

#[tokio::test(start_paused = true)]
async fn expired_snapshots_release_evicted_views_and_reconnects_receive_current_state() {
    let hub = Hub::new();
    for cycle in 0..8 {
        hub.publish_view(Fixture::view(0, &"x".repeat(1000)))
            .unwrap();
        let (snapshot, views) = hub.subscribe_views();
        let (state, state_rx) = hub.subscribe_state();
        let held = Arc::downgrade(&snapshot[0]);
        let (server, _non_reader) = tokio::io::duplex(64);
        let connection = ShellConnection::new(
            server,
            views,
            state_rx,
            hub.clone(),
            Err(Refusal::denied("test")),
            None,
        );
        let surface = snapshot[0].surface.clone();
        hub.drop_surface(&surface);
        for sequence in 0..70 {
            hub.publish_view(Fixture::view(0, &format!("cycle{cycle}-{sequence}")))
                .unwrap();
        }
        let start = Instant::now();
        assert!(connection.stream(snapshot, state).await.is_err());
        assert_eq!(Instant::now() - start, Duration::from_secs(5));
        assert!(
            held.upgrade().is_none(),
            "expired snapshot retains an evicted tree"
        );

        let mut healthy = Fixture::new(&hub);
        let (snapshot, _) = hub.subscribe_views();
        let expected_revision = snapshot[0].view.revision;
        let (served, ()) =
            tokio::join!(healthy.connection.stream(snapshot, hub.snapshot()), async {
                let mut line = String::new();
                healthy.reader.read_line(&mut line).await.unwrap();
                let value: serde_json::Value = serde_json::from_str(&line).unwrap();
                assert_eq!(value["view"]["root"]["key"], format!("cycle{cycle}-69"));
                assert_eq!(value["view"]["revision"], expected_revision.to_string());
                drop(healthy.reader);
            });
        served.unwrap();
    }
}
