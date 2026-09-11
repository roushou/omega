use super::*;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};

struct Server {
    socket: Socket,
    task: tokio::task::JoinHandle<Result<(), ShellError>>,
}
impl Server {
    fn start() -> Self {
        let path = omega_host::TempPath::sibling(
            &std::env::temp_dir().join("omega-shell-budget.sock"),
            "test",
        );
        let socket = Socket::at(path);
        let server = ShellServer::bind_at(socket.clone(), Hub::new()).unwrap();
        let task = tokio::spawn(async move { server.run().await });
        Self { socket, task }
    }
    async fn connect(&self) -> BufReader<UnixStream> {
        let mut stream = self.socket.connect_stream().await.unwrap();
        stream.write_all(b"{}\n").await.unwrap();
        BufReader::new(stream)
    }
    async fn answered(reader: &mut BufReader<UnixStream>) {
        let mut line = String::new();
        tokio::time::timeout(Duration::from_secs(1), reader.read_line(&mut line))
            .await
            .unwrap()
            .unwrap();
        let frame: Frame = serde_json::from_str(&line).unwrap();
        assert!(Refusal::of(&frame).is_some());
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.task.abort();
        let _ = std::fs::remove_file(self.socket.path());
    }
}

#[tokio::test(start_paused = true)]
async fn saturated_observers_wait_for_capacity_and_closed_connections_release_it() {
    let server = Server::start();
    let mut clients = Vec::new();
    for _ in 0..ShellServer::CONNECTION_LIMIT {
        let mut client = server.connect().await;
        Server::answered(&mut client).await;
        clients.push(client);
    }
    let mut waiting = server.connect().await;
    let mut line = String::new();
    assert!(
        tokio::time::timeout(Duration::from_millis(1), waiting.read_line(&mut line))
            .await
            .is_err()
    );
    drop(clients.pop());
    Server::answered(&mut waiting).await;
    drop(server);
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(1), waiting.read_line(&mut line))
            .await
            .unwrap()
            .unwrap(),
        0
    );
}

#[tokio::test(start_paused = true)]
async fn stalled_observer_writes_have_a_deadline() {
    let (stream, _non_reader) = UnixStream::pair().unwrap();
    let hub = Hub::new();
    let (_, views) = hub.subscribe_views();
    let (_, state) = hub.subscribe_state();
    let mut connection = ShellConnection::new(
        stream,
        views,
        state,
        hub,
        Err(Refusal::denied("test")),
        None,
    );
    assert!(matches!(
        connection
            .write("x".repeat(omega_proto::MAX_FRAME_LEN))
            .await,
        Err(ShellError::WriteTimeout)
    ));
}

#[tokio::test]
async fn an_unterminated_oversized_request_closes_only_its_connection() {
    let server = Server::start();
    let mut bad = server.socket.connect_stream().await.unwrap();
    let oversized = vec![b'x'; omega_proto::MAX_FRAME_LEN + 1];
    // The receiver may close before write_all finishes accepting the final chunk.
    let _ = tokio::time::timeout(Duration::from_secs(2), bad.write_all(&oversized))
        .await
        .unwrap();
    use tokio::io::AsyncReadExt;
    let mut byte = [0];
    let read = tokio::time::timeout(Duration::from_secs(2), bad.read(&mut byte))
        .await
        .unwrap();
    assert!(matches!(read, Ok(0)) || read.is_err());
    let mut healthy = server.connect().await;
    Server::answered(&mut healthy).await;
}

struct SlowBroker(Arc<tokio::sync::Notify>);
#[async_trait::async_trait]
impl omega_brokers::Broker for SlowBroker {
    fn name(&self) -> &'static str {
        "slow"
    }
    fn topics(&self) -> &'static [omega_proto::SystemTopic] {
        &[]
    }
    fn actions(&self) -> &'static [omega_proto::ActionKind] {
        &[omega_proto::ActionKind::Lock]
    }
    async fn act(
        &mut self,
        _: &omega_proto::omega::action::Kind,
    ) -> Result<Option<omega_proto::omega::StatePatch>, omega_brokers::BrokerError> {
        self.0.notify_one();
        std::future::pending().await
    }
}

#[tokio::test]
async fn slow_observation_actions_do_not_delay_subscription_changes_or_views() {
    use omega_proto::omega::{Act, Action, Lock, Subscribe, action, invoke};
    let (server, client) = UnixStream::pair().unwrap();
    let peer = Peer::operator(std::process::id() as i32, server.peer_cred().unwrap().uid());
    let hub = Hub::new();
    let units = UnitTable::detached(hub.clone());
    let shutdown = crate::Shutdown::new();
    let brokers = Brokerage::new(hub.clone(), shutdown.clone());
    let entered = Arc::new(tokio::sync::Notify::new());
    brokers.add(Box::new(SlowBroker(entered.clone())));
    let supervisor = Supervisor::new(Socket::at("/unused"), units.clone(), shutdown.clone());
    let (snapshot, state) = hub.subscribe_state();
    let (views, updates) = hub.subscribe_views();
    let connection = ShellConnection::new(
        server,
        updates,
        state,
        hub.clone(),
        peer,
        Some(Gateway {
            layout: None,
            supervisor,
            units,
            brokers: brokers.clone(),
        }),
    );
    let task = tokio::spawn(connection.stream(views, snapshot));
    let mut client = BufReader::new(client);
    let request = Frame {
        stream_id: 1,
        body: Some(frame::Body::Invoke(Invoke {
            op: Some(invoke::Op::Act(Act {
                action: Some(Action {
                    kind: Some(action::Kind::Lock(Lock {})),
                }),
            })),
        })),
    };
    client
        .get_mut()
        .write_all(
            (Observation::line(&request).unwrap()
                + "
")
            .as_bytes(),
        )
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(2), entered.notified())
        .await
        .unwrap();
    let subscribe = Frame {
        stream_id: 3,
        body: Some(frame::Body::Invoke(Invoke {
            op: Some(invoke::Op::Subscribe(Subscribe {
                replace: true,
                ..Default::default()
            })),
        })),
    };
    client
        .get_mut()
        .write_all(
            (Observation::line(&subscribe).unwrap()
                + "
")
            .as_bytes(),
        )
        .await
        .unwrap();
    let mut line = String::new();
    tokio::time::timeout(Duration::from_secs(2), client.read_line(&mut line))
        .await
        .unwrap()
        .unwrap();
    let reply: Frame = serde_json::from_str(&line).unwrap();
    assert_eq!(reply.stream_id, 3);
    assert!(Refusal::of(&reply).is_none());
    hub.publish_view(ViewUpdate {
        surface: crate::hub::SurfaceRef::new(
            omega_proto::UnitName::parse("example").unwrap(),
            omega_proto::SurfaceId::parse("view").unwrap(),
        ),
        view: omega_proto::omega::ViewTree::default(),
    })
    .unwrap();
    line.clear();
    tokio::time::timeout(Duration::from_secs(2), client.read_line(&mut line))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&line).unwrap()["surface"],
        "view"
    );
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    shutdown.trigger();
    brokers.stop().await;
}

#[tokio::test]
async fn observation_writes_drain_large_requests_without_dispatching_them_early() {
    let (server, client) = UnixStream::pair().unwrap();
    let hub = Hub::new();
    let (_, views) = hub.subscribe_views();
    let (_, state) = hub.subscribe_state();
    let mut connection = ShellConnection::new(
        server,
        views,
        state,
        hub,
        Err(Refusal::denied("test")),
        None,
    );
    let mut client = BufReader::new(client);
    let request = " ".repeat(900_000);
    let (written, received) = tokio::join!(connection.write("x".repeat(600_000)), async {
        client
            .get_mut()
            .write_all(
                (request.clone()
                    + "
")
                .as_bytes(),
            )
            .await
            .unwrap();
        let mut response = String::new();
        client.read_line(&mut response).await.unwrap();
        response
    },);
    written.unwrap();
    assert_eq!(received.len(), 600_001);
    assert_eq!(
        connection.requests.next_line().await.unwrap().unwrap(),
        request
    );
}

#[tokio::test]
async fn lag_recovery_removes_large_views_using_only_address_and_revision() {
    tokio::time::timeout(Duration::from_secs(5), async {
        use crate::hub::SurfaceRef;
        use omega_proto::omega::{ViewNode, ViewTree};
        use omega_proto::{SurfaceId, UnitName};

        let hub = Hub::new();
        let unit = UnitName::parse("retired").unwrap();
        let surface = SurfaceRef::new(unit.clone(), SurfaceId::parse("panel").unwrap());
        hub.publish_view(ViewUpdate {
            surface: surface.clone(),
            view: ViewTree {
                root: Some(ViewNode {
                    key: "x".repeat(600_000),
                    ..Default::default()
                }),
                revision: 0,
            },
        })
        .unwrap();
        let (snapshot, views) = hub.subscribe_views();
        let (_, state) = hub.subscribe_state();
        let (server, client) = UnixStream::pair().unwrap();
        let mut client = BufReader::new(client);
        let mut connection = ShellConnection::new(
            server,
            views,
            state,
            hub.clone(),
            Err(Refusal::denied("test")),
            None,
        );
        let (written, line) = tokio::join!(connection.write_views(snapshot.clone()), async {
            let mut line = String::new();
            client.read_line(&mut line).await.unwrap();
            line
        });
        written.unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&line).unwrap(),
            serde_json::to_value(snapshot[0].as_ref()).unwrap()
        );
        let revision = snapshot[0].view.revision;
        assert_eq!(connection.sent_views.get(&surface), Some(&revision));
        drop(snapshot);

        hub.forget_unit(&unit);
        let remaining = SurfaceRef::new(
            UnitName::parse("current").unwrap(),
            SurfaceId::parse("panel").unwrap(),
        );
        for index in 0..70 {
            hub.publish_view(ViewUpdate {
                surface: remaining.clone(),
                view: ViewTree {
                    root: Some(ViewNode {
                        key: index.to_string(),
                        ..Default::default()
                    }),
                    revision: 0,
                },
            })
            .unwrap();
        }
        assert!(matches!(
            connection.views.recv().await,
            Err(broadcast::error::RecvError::Lagged(_))
        ));
        let (written, lines) = tokio::join!(connection.resync_views(), async {
            let mut lines = Vec::new();
            for _ in 0..2 {
                let mut line = String::new();
                client.read_line(&mut line).await.unwrap();
                lines.push(serde_json::from_str::<serde_json::Value>(&line).unwrap());
            }
            lines
        });
        written.unwrap();
        assert_eq!(lines[0]["unit"], "retired");
        assert!(lines[0]["view"]["root"].is_null());
        assert_eq!(lines[0]["view"]["revision"], (revision + 1).to_string());
        assert_eq!(lines[1]["unit"], "current");
        assert_eq!(lines[1]["view"]["root"]["key"], "69");
        assert!(!connection.sent_views.contains_key(&surface));
        assert_eq!(connection.sent_views.len(), 1);
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn a_blocked_observer_does_not_hold_up_another_observers_large_view() {
    tokio::time::timeout(Duration::from_secs(3), async {
        let hub = Hub::new();
        let update = ViewUpdate {
            surface: crate::hub::SurfaceRef::new(
                omega_proto::UnitName::parse("example").unwrap(),
                omega_proto::SurfaceId::parse("panel").unwrap(),
            ),
            view: omega_proto::omega::ViewTree {
                root: Some(omega_proto::omega::ViewNode {
                    key: "x".repeat(600_000),
                    ..Default::default()
                }),
                revision: 1,
            },
        };
        let updates = [Arc::new(update)];
        let (slow_server, _non_reader) = UnixStream::pair().unwrap();
        let (fast_server, fast_client) = UnixStream::pair().unwrap();
        let mut client = BufReader::new(fast_client);
        let mut connections = [slow_server, fast_server].map(|stream| {
            ShellConnection::new(stream, hub.subscribe_views().1, hub.subscribe_state().1,
                hub.clone(), Err(Refusal::denied("test")), None)
        });
        let [slow, fast] = &mut connections;
        tokio::select! {
            biased;
            result = slow.write_views(updates.clone()) => panic!("non-reader unexpectedly completed: {result:?}"),
            _ = async {
                let (written, line) = tokio::join!(fast.write_views(updates.clone()), async {
                    let mut line = String::new();
                    client.read_line(&mut line).await.unwrap();
                    line
                });
                written.unwrap();
                assert_eq!(serde_json::from_str::<serde_json::Value>(&line).unwrap(), serde_json::to_value(updates[0].as_ref()).unwrap());
            } => {}
        }
        assert!(slow.sent_views.is_empty());
        assert_eq!(fast.sent_views.get(&updates[0].surface), Some(&1));
    }).await.unwrap();
}

mod pressure;
