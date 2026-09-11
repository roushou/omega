//! The observation socket: what an observer is sent, and what it can decline.
//!
//! Reading it needs no handshake, so the default is everything the daemon
//! holds. What was missing is the other half — an observer that only draws
//! views had no way to say so, and was sent every state topic there is
//! whatever it asked for.

mod common;

use std::time::Duration;

use common::{TempSocket, widget_manifest};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use omega_daemon::Shutdown;
use omega_daemon::broker::Brokerage;
use omega_daemon::hub::Hub;
use omega_daemon::manifest::ManifestStore;
use omega_daemon::shell::ShellServer;
use omega_daemon::supervisor::Supervisor;
use omega_daemon::units::UnitTable;
use omega_proto::Socket;
use omega_proto::omega::{BatteryState, StatePatch, StateTopic, state_topic};

/// How long a line that is coming should take. Generous: what is being
/// asserted is that it arrives, not how fast.
const PROMPTLY: Duration = Duration::from_secs(5);

/// How long to give a line that should never come. Short, because every one
/// of these is spent waiting on purpose.
const SETTLES: Duration = Duration::from_millis(750);

/// Long enough for a heartbeat to fall due, whatever the socket's cadence.
const BEAT_PATIENCE: Duration = Duration::from_secs(15);

/// An observer, and the daemon it is watching.
struct Observing {
    hub: Hub,
    reader: BufReader<tokio::net::UnixStream>,
    _socket: TempSocket,
}

impl Observing {
    /// A shell server that serves requests as well as streaming, which is
    /// what a shell drawing a button already is.
    async fn new(tag: &str) -> Self {
        let hub = Hub::new();
        let held = TempSocket::new(tag);
        let socket = held.socket();

        let units = UnitTable::detached(hub.clone());
        units.adopt(&ManifestStore::from_manifests([widget_manifest(
            "battery-widget",
            "battery",
        )]));
        let supervisor = Supervisor::new(
            Socket::at(socket.path().with_extension("control")),
            units.clone(),
            Shutdown::new(),
        );
        let brokers = Brokerage::new(hub.clone(), Shutdown::new());

        let server = ShellServer::bind_at(socket.clone(), hub.clone())
            .unwrap()
            .serving(supervisor, units, brokers);
        tokio::spawn(async move {
            let _ = server.run().await;
        });

        let stream = socket.connect_stream().await.unwrap();
        Self {
            hub,
            reader: BufReader::new(stream),
            _socket: held,
        }
    }

    /// Ask for exactly these topics and nothing else.
    ///
    /// Returns once the daemon has *answered*, not merely once a line has
    /// arrived: the snapshot it sends on connect comes first, and going on
    /// before the request is served would race the subscription against
    /// whatever the test publishes next.
    async fn subscribe_to(&mut self, topics: &[&str]) {
        let names = topics
            .iter()
            .map(|topic| format!("{topic:?}"))
            .collect::<Vec<_>>()
            .join(",");
        let request = format!(
            r#"{{"streamId":1,"invoke":{{"subscribe":{{"topics":[{names}],"events":[],"replace":true}}}}}}"#
        );
        self.reader
            .get_mut()
            .write_all(format!("{request}\n").as_bytes())
            .await
            .unwrap();

        loop {
            let line = self.line(PROMPTLY).await;
            if line["result"].is_object() {
                assert!(
                    line["result"]["error"].is_null(),
                    "the daemon refused the subscription: {line}"
                );
                return;
            }
        }
    }

    /// The next line, whatever it is.
    async fn line(&mut self, patience: Duration) -> serde_json::Value {
        let mut line = String::new();
        tokio::time::timeout(patience, self.reader.read_line(&mut line))
            .await
            .expect("the daemon said nothing at all")
            .unwrap();
        serde_json::from_str(line.trim()).unwrap()
    }

    /// Whether this topic arrives within `patience`, skipping any others.
    ///
    /// The daemon sends everything it holds on connect, so a test looking for
    /// one topic has to read past the rest rather than assume it comes first.
    async fn sees(&mut self, topic: &str, patience: Duration) -> bool {
        let deadline = tokio::time::Instant::now() + patience;
        loop {
            let left = deadline.saturating_duration_since(tokio::time::Instant::now());
            match self.topic_within(left).await {
                Some(seen) if seen == topic => return true,
                Some(_) => continue,
                None => return false,
            }
        }
    }

    /// The first state topic to arrive, or `None` if none does in time.
    ///
    /// Waiting for a marker line instead would race it against the topic:
    /// views and state are separate broadcasts, and which the connection
    /// picks up first is the runtime's business.
    async fn topic_within(&mut self, patience: Duration) -> Option<String> {
        let deadline = tokio::time::Instant::now() + patience;

        loop {
            let left = deadline.saturating_duration_since(tokio::time::Instant::now());
            let mut line = String::new();
            match tokio::time::timeout(left, self.reader.read_line(&mut line)).await {
                // Nothing more is coming, which for a narrowed observer is
                // the whole assertion.
                Err(_) | Ok(Ok(0)) => return None,
                Ok(Err(e)) => panic!("the observer connection broke: {e}"),
                Ok(Ok(_)) => {
                    let line: serde_json::Value = serde_json::from_str(line.trim()).unwrap();
                    if let Some(topic) = line["topic"].as_str() {
                        return Some(topic.to_string());
                    }
                }
            }
        }
    }
}

fn battery(charge: f64) -> StatePatch {
    StatePatch {
        topics: vec![StateTopic {
            topic: "battery".into(),
            revision: 0,
            value: Some(state_topic::Value::Battery(BatteryState {
                level: charge,
                ..Default::default()
            })),
        }],
    }
}

#[tokio::test]
async fn an_observer_that_asks_for_nothing_is_sent_everything() {
    // Reading this socket needs no handshake, so a peer that never speaks —
    // `omega status`, a raw `socat` — still sees what the daemon holds.
    let mut observing = Observing::new("observe-default").await;

    observing.hub.publish_state(battery(0.9)).unwrap();

    assert!(
        observing.sees("battery", PROMPTLY).await,
        "an observer that asked for nothing was not sent the battery"
    );
}

#[tokio::test]
async fn an_observer_can_decline_every_topic_and_still_draw() {
    // What the shell is: it renders views and reads no state at all. It had
    // no way to say so, and carried every topic the daemon published.
    let mut observing = Observing::new("observe-none").await;
    observing.subscribe_to(&[]).await;

    observing.hub.publish_state(battery(0.9)).unwrap();

    assert_eq!(
        observing.topic_within(SETTLES).await,
        None,
        "a topic reached an observer that asked for none"
    );
}

#[tokio::test]
async fn an_observer_is_sent_the_topics_it_named_and_no_others() {
    // `omega status` wants the supervisor's report and nothing else.
    let mut observing = Observing::new("observe-some").await;
    observing.subscribe_to(&["units"]).await;

    observing.hub.publish_state(battery(0.9)).unwrap();

    assert_eq!(
        observing.topic_within(SETTLES).await,
        None,
        "an unnamed topic reached an observer that named another"
    );
}

#[tokio::test]
async fn a_quiet_daemon_still_says_it_is_there() {
    // An observer cannot tell a quiet daemon from a dead one — a peer that
    // goes away leaves the socket reading connected — so the shell watches
    // for silence and reconnects through it. Before the heartbeat that made
    // incidental traffic load-bearing, and an observer that narrowed its
    // subscription had turned its own keepalive off.
    let mut observing = Observing::new("observe-quiet").await;
    observing.subscribe_to(&[]).await;

    // Nothing is published from here on, and the observer reads no topics
    // even if something were: the only thing left that can arrive is the beat.
    let line = observing.line(BEAT_PATIENCE).await;
    assert_eq!(
        line["heartbeat"].as_bool(),
        Some(true),
        "a socket with nothing to report said nothing at all: {line}"
    );
}

#[test]
fn a_heartbeat_is_a_line_an_observer_can_tell_apart() {
    // Every line on this socket is a JSON object told apart by its keys, so
    // the beat must not look like a view or a topic to a reader that only
    // checks for one.
    let line = serde_json::to_value(omega_daemon::hub::Heartbeat { heartbeat: true }).unwrap();

    assert_eq!(line["heartbeat"].as_bool(), Some(true));
    assert!(line["view"].is_null());
    assert!(line["topic"].is_null());
}
