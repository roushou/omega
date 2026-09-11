//! One connection: admission, then the state-mirror loop.

pub mod admission;
pub mod dispatch;
pub mod liveness;
pub(crate) mod operations;
use operations::Operations;
pub mod subscriptions;

use std::collections::HashMap;

use tokio::net::UnixStream;
use tokio::sync::{broadcast, oneshot};

use omega_proto::omega::{Event, Frame, Hello, Ping, Pong, StatePatch, Welcome, frame, result};
use omega_proto::{Handshake, HandshakeError, PROTOCOL_VERSION, Refusal, Transport};

use crate::broker::Brokerage;
use crate::hub::Hub;
use crate::shutdown::Shutdown;
use crate::supervisor::Supervisor;
use crate::units::{DaemonStreams, Request, UnitTable};
use omega_proto::CodecError;

pub use admission::{Peer, Role};
pub use dispatch::{Dispatcher, OpKind, Response};
pub use liveness::{Health, Liveness};
pub use subscriptions::Subscriptions;

// Caller cancellation does not cancel execution in the unit. Keep admission
// bytes and a pending slot until a terminal reply or connection teardown.
struct PendingRequest {
    answer: oneshot::Sender<Result<result::Outcome, Refusal>>,
    _bytes: tokio::sync::OwnedSemaphorePermit,
}

/// Serves a single connection: admission, then the state-mirror loop.
#[derive(Debug)]
pub struct Session {
    supervisor: Supervisor,
    hub: Hub,
    liveness: Liveness,
    shutdown: Shutdown,
    /// Where a connected unit registers itself, so the daemon can invoke it.
    units: UnitTable,
    /// The subsystems this daemon brokers. Empty by default, which answers
    /// every brokered action `UNIMPLEMENTED` — right for a session that is
    /// not a daemon, and for a test that is only exercising the protocol.
    brokers: Brokerage,
    layout: Option<omega_host::Layout>,
}

impl Session {
    pub fn new(supervisor: Supervisor, hub: Hub) -> Self {
        let shutdown = Shutdown::new();
        Self {
            supervisor,
            layout: None,
            brokers: Brokerage::new(hub.clone(), shutdown.clone()),
            units: UnitTable::detached(hub.clone()),
            hub,
            liveness: Liveness::new(),
            shutdown,
        }
    }

    pub fn with_layout(mut self, layout: omega_host::Layout) -> Self {
        self.layout = Some(layout);
        self
    }

    /// Join the brokers the daemon runs, so an action can reach one.
    pub fn with_brokers(mut self, brokers: Brokerage) -> Self {
        self.brokers = brokers;
        self
    }

    /// Join the table the daemon invokes units through.
    pub fn with_units(mut self, units: UnitTable) -> Self {
        self.units = units;
        self
    }

    /// End this session when the daemon shuts down.
    pub fn with_shutdown(mut self, shutdown: Shutdown) -> Self {
        self.shutdown = shutdown;
        self
    }

    /// A session with a different keepalive cadence (tests, unusual peers).
    pub fn with_liveness(mut self, liveness: Liveness) -> Self {
        self.liveness = liveness;
        self
    }

    /// Serve one connection until the peer goes away.
    ///
    /// A peer that is turned away is *told* — the refusal goes out as a frame
    /// before the socket closes, so a unit can report why it was rejected
    /// instead of seeing an unexplained EOF.
    pub async fn serve(self, stream: UnixStream) -> Result<(), SessionError> {
        let shutdown = self.shutdown.clone();
        tokio::select! {
            biased;
            _ = shutdown.wait() => Ok(()),
            result = self.serve_until_closed(stream) => result,
        }
    }

    async fn serve_until_closed(self, stream: UnixStream) -> Result<(), SessionError> {
        // The kernel's word on who this is: a pid the peer cannot forge, and
        // the uid that decides whether it may be the operator.
        let credentials = stream.peer_cred().ok();
        let pid = credentials.as_ref().and_then(|c| c.pid()).unwrap_or(0);
        let uid = credentials.map(|c| c.uid()).unwrap_or(u32::MAX);
        let mut connection = Transport::new(stream).duplex();

        let hello = match Self::hello(&mut connection).await {
            Ok(hello) => hello,
            Err(e) => {
                let refusal = Refusal::precondition(e.to_string());
                let _ = connection.send(refusal.frame(0)).await;
                return Err(e);
            }
        };

        let handover = self.supervisor.handover().await;
        let peer = match self.admit(pid, uid, &hello) {
            Ok(peer) => peer,
            Err(refusal) => {
                tracing::warn!(pid, code = ?refusal.code, "connection refused: {}", refusal.message);
                let _ = connection.send(refusal.frame(0)).await;
                return Err(SessionError::Refused(refusal));
            }
        };

        tracing::info!(
            pid,
            unit = %peer.label(),
            manifest_hash = %hello.manifest_hash,
            "handshake complete"
        );

        let peer = std::sync::Arc::new(peer);

        // What this peer may see, fixed at admission from its manifest.
        let mut subscriptions = self.subscriptions(&peer);

        // Snapshot and subscription are taken atomically, so nothing published
        // in between is lost.
        let (snapshot, mut state) = self.hub.subscribe_state();
        let snapshot = subscriptions.filter_snapshot(snapshot);
        let mut events = self.hub.subscribe_events();

        // A unit is reachable by name for as long as this session lasts.
        let (outbound, mut requests) = tokio::sync::mpsc::channel::<Request>(16);
        let registered = peer
            .unit_name()
            .map(|name| self.units.connected(name, outbound));

        let settings = peer
            .unit_name()
            .map(|name| self.units.config(name))
            .unwrap_or_default();
        drop(handover);

        connection
            .send(Frame {
                stream_id: 0,
                body: Some(frame::Body::Welcome(Welcome {
                    protocol_version: PROTOCOL_VERSION,
                    unit_id: peer.label(),
                    daemon_version: env!("CARGO_PKG_VERSION").to_string(),
                    capabilities: peer.grants().wire(),
                    state: Some(snapshot),
                    // What the document configured this unit with. It arrives
                    // at the handshake because a plugin's fields are built out
                    // of it: there is no moment later than construction at
                    // which handing it over would mean anything.
                    config: settings,
                })),
            })
            .await?;

        let mut streams = DaemonStreams::new();
        let mut pending: HashMap<u64, PendingRequest> = HashMap::new();

        let dispatcher = std::sync::Arc::new(
            Dispatcher::new(
                self.hub.clone(),
                self.supervisor.clone(),
                self.units.clone(),
                self.brokers.clone(),
            )
            .with_layout(self.layout.clone()),
        );
        let mut executing = Operations::new();
        let mut liveness = self.liveness.clone();
        let mut keepalive = tokio::time::interval(liveness.interval());
        keepalive.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        keepalive.tick().await; // the first tick is immediate

        loop {
            tokio::select! {
                published = state.recv() => {
                    match published {
                        // Only what this peer subscribed to: a unit is woken
                        // for the topics it declared, not for every source on
                        // the machine.
                        Ok(patch) => {
                            if let Some(patch) = subscriptions.filter(&patch) {
                                connection.send(Self::patch_frame(patch)).await?;
                            }
                        }
                        // The mirror is now behind by an unknown amount, and a
                        // silently stale mirror is worse than a slow one: send
                        // the whole truth rather than let it drift forever.
                        Err(broadcast::error::RecvError::Lagged(missed)) => {
                            tracing::warn!(unit = %peer.label(), missed, "state subscriber lagged; resyncing");
                            let (snapshot, receiver) = self.hub.subscribe_state();
                            state = receiver;
                            if let Some(patch) = subscriptions.filter(&StatePatch { topics: snapshot.topics }) {
                                connection.send(Self::patch_frame(patch)).await?;
                            }
                        }
                        Err(broadcast::error::RecvError::Closed) => return Ok(()),
                    }
                }
                event = events.recv() => {
                    match event {
                        // An event is a moment, not a value: a unit that was
                        // not listening missed it, and there is nothing to
                        // resend.
                        Ok(event) if subscriptions.wants_event(&event) => {
                            if let Some(patch) = subscriptions.filter(&self.snapshot_patch()) {
                                connection.send(Self::patch_frame(patch)).await?;
                            }
                            connection.send(Self::event_frame(event)).await?;
                        }
                        Ok(_) => {}
                        Err(broadcast::error::RecvError::Lagged(missed)) => {
                            tracing::warn!(unit = %peer.label(), missed, "event subscriber lagged");
                        }
                        Err(broadcast::error::RecvError::Closed) => return Ok(()),
                    }
                }
                completed = executing.next(), if !executing.is_empty() => {
                    let frame = completed.map_err(|error| SessionError::Transport(CodecError::Io(std::io::Error::other(error))))?;
                    connection.send(frame).await?;
                }
                // The daemon asking this unit for something.
                Some(request) = requests.recv() => {
                    if request.answer.is_closed() { continue; }
                    if pending.len() >= 16 {
                        let _ = request.answer.send(Err(Refusal::exhausted("too many pending requests")));
                        continue;
                    }
                    let stream_id = streams.allocate();
                    pending.insert(stream_id, PendingRequest { answer: request.answer, _bytes: request._bytes });
                    connection.send(Frame {
                        stream_id,
                        body: Some(frame::Body::Invoke(omega_proto::omega::Invoke {
                            op: Some(request.op),
                        })),
                    }).await?;
                }
                received = connection.recv() => {
                    match received {
                        Ok(Some(frame)) => {
                            liveness.seen();
                            if Self::answers_daemon(&frame, &mut pending) {
                                continue;
                            }
                            if let Some(frame::Body::Invoke(invoke)) = &frame.body
                                && Operations::deferred(invoke)
                            {
                                if let Err(refusal) = executing.start(
                                    dispatcher.clone(), peer.clone(), subscriptions.clone(),
                                    frame.stream_id, invoke.clone(),
                                ) {
                                    connection.send(refusal.frame(frame.stream_id)).await?;
                                }
                                continue;
                            }
                            self.handle(&dispatcher, &peer, &mut subscriptions, &frame, &mut connection).await?;
                        }
                        Ok(None) => return Ok(()),
                        Err(e) => return Err(SessionError::Transport(e)),
                    }
                }
                _ = async {
                    match &registered {
                        Some(guard) => guard.cancelled().await,
                        None => std::future::pending().await,
                    }
                } => return Ok(()),
                _ = keepalive.tick() => {
                    if liveness.health() == Health::Unresponsive {
                        tracing::warn!(unit = %peer.label(), "peer stopped answering; closing");
                        return Err(SessionError::Unresponsive(peer.label()));
                    }
                    connection.send(Self::ping()).await?;
                }
            }
        }
    }

    /// The topics a peer may see: what its manifest declares, or — for a
    /// debug client the operator admitted on purpose — everything, read-only.
    fn subscriptions(&self, peer: &Peer) -> Subscriptions {
        peer.unit_name()
            .and_then(|name| self.units.manifest(name).map(|unit| (name, unit)))
            .map_or_else(Subscriptions::watcher, |(name, unit)| {
                Subscriptions::of(name, &unit.manifest)
            })
    }

    /// The peer's opening frame, or why it is not one.
    async fn hello(
        connection: &mut omega_proto::Duplex<UnixStream>,
    ) -> Result<Hello, SessionError> {
        let frame = tokio::time::timeout(Handshake::TIMEOUT, connection.recv())
            .await
            .map_err(|_| HandshakeError::Timeout("Hello"))??;
        Ok(Handshake::expect_hello(frame)?)
    }

    /// Identity, then grants. Both come from the daemon's own records: the
    /// supervisor says which unit owns this (pid, token), and the manifest on
    /// disk says what that unit may do. Nothing here is taken from `Hello`
    /// except the claim being checked.
    fn admit(&self, pid: i32, uid: u32, hello: &Hello) -> Result<Peer, Refusal> {
        let Some(name) = self.supervisor.identify(pid, &hello.token) else {
            return Peer::operator(pid, uid);
        };

        let unit = self
            .units
            .manifest(&name)
            .ok_or_else(|| Refusal::precondition(format!("no manifest on file for {name}")))?;

        if hello.manifest_hash != unit.hash {
            return Err(Refusal::precondition(format!(
                "manifest hash mismatch for {name}: expected {}, got {}",
                unit.hash, hello.manifest_hash
            )));
        }

        Peer::unit(name, &unit.manifest)
    }

    /// Everything the daemon knows, as one patch.
    fn snapshot_patch(&self) -> StatePatch {
        StatePatch {
            topics: self.hub.snapshot().topics,
        }
    }

    fn patch_frame(patch: StatePatch) -> Frame {
        Frame {
            stream_id: 0,
            body: Some(frame::Body::StatePatch(patch)),
        }
    }

    /// Route a `Result` back to whatever asked for it. Only streams the
    /// daemon allocated are its own answers; a unit's `Result` on an odd
    /// stream is a reply to nothing it asked.
    fn answers_daemon(frame: &Frame, pending: &mut HashMap<u64, PendingRequest>) -> bool {
        if !DaemonStreams::is_ours(frame.stream_id) {
            return false;
        }
        let Some(frame::Body::Result(result)) = &frame.body else {
            return false;
        };
        if !result.done {
            return pending.contains_key(&frame.stream_id);
        }
        let Some(request) = pending.remove(&frame.stream_id) else {
            return false;
        };
        let outcome = match Refusal::of(frame) {
            Some(refusal) => Err(refusal),
            None => match frame.body.as_ref() {
                Some(frame::Body::Result(result)) => match result.outcome.clone() {
                    Some(outcome) => Ok(outcome),
                    None => Err(Refusal::invalid("Result carries no outcome")),
                },
                _ => Err(Refusal::invalid("expected a Result")),
            },
        };

        let _ = request.answer.send(outcome);
        true
    }

    fn event_frame(event: Event) -> Frame {
        Frame {
            stream_id: 0,
            body: Some(frame::Body::Event(event)),
        }
    }

    fn ping() -> Frame {
        Frame {
            stream_id: 0,
            body: Some(frame::Body::Ping(Ping {
                nonce: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_nanos() as u64)
                    .unwrap_or_default(),
            })),
        }
    }

    /// Answer one frame: every invocation gets a `Result` — an outcome or a
    /// refusal — on the stream that carried it.
    async fn handle(
        &self,
        dispatcher: &Dispatcher,
        peer: &Peer,
        subscriptions: &mut Subscriptions,
        frame: &Frame,
        connection: &mut omega_proto::Duplex<UnixStream>,
    ) -> Result<(), SessionError> {
        let answer = match frame.body.as_ref() {
            Some(frame::Body::Invoke(invoke)) => {
                dispatcher.invoke(peer, subscriptions, invoke).await
            }
            Some(frame::Body::Hello(_)) => Err(Refusal::invalid("Hello after the handshake")),
            // A peer may check on the daemon too.
            Some(frame::Body::Ping(ping)) => {
                connection
                    .send(Frame {
                        stream_id: frame.stream_id,
                        body: Some(frame::Body::Pong(Pong { nonce: ping.nonce })),
                    })
                    .await?;
                return Ok(());
            }
            // The answer to our own keepalive; receiving it was the point.
            Some(frame::Body::Pong(_)) => return Ok(()),
            other => {
                tracing::debug!(?other, "ignoring frame from peer");
                return Ok(());
            }
        };

        match answer {
            Ok(response) => connection.send(response.frame(frame.stream_id)).await?,
            Err(refusal) => {
                tracing::warn!(
                    unit = %peer.label(),
                    code = ?refusal.code,
                    "refused: {}", refusal.message
                );
                connection.send(refusal.frame(frame.stream_id)).await?;
            }
        }
        Ok(())
    }
}

/// Why a session ended.
#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error("handshake failed: {0}")]
    Handshake(#[from] HandshakeError),
    #[error("transport error: {0}")]
    Transport(#[from] CodecError),
    /// The peer was turned away. It was told why before the socket closed.
    #[error("refused: {0}")]
    Refused(#[from] Refusal),
    /// The peer stopped answering keepalives.
    #[error("{0} stopped answering")]
    Unresponsive(String),
}
