//! One connection: admission, then the state-mirror loop.

pub mod admission;
pub mod dispatch;
pub mod liveness;
pub mod subscriptions;

use std::collections::HashMap;

use tokio::net::UnixStream;
use tokio::sync::{broadcast, oneshot};

use omega_proto::omega::{Event, Frame, Hello, Ping, Pong, StatePatch, Welcome, frame, result};
use omega_proto::{
    Handshake, HandshakeError, PROTOCOL_VERSION, ReadHalf, Refusal, Transport, WriteHalf,
};

use crate::error::SessionError;
use crate::hub::Hub;
use crate::shutdown::Shutdown;
use crate::supervisor::Supervisor;
use crate::units::{DaemonStreams, Request, UnitTable};

pub use admission::{Peer, Role};
pub use dispatch::{Dispatcher, OpKind, Response};
pub use liveness::{Health, Liveness};
pub use subscriptions::Subscriptions;

/// Serves a single connection: admission, then the state-mirror loop.
#[derive(Debug)]
pub struct Session {
    supervisor: Supervisor,
    hub: Hub,
    liveness: Liveness,
    shutdown: Shutdown,
    /// Where a connected unit registers itself, so the daemon can invoke it.
    units: UnitTable,
}

impl Session {
    pub fn new(supervisor: Supervisor, hub: Hub) -> Self {
        let hub_for_table = hub.clone();
        Self {
            supervisor,
            hub,
            liveness: Liveness::new(),
            shutdown: Shutdown::new(),
            units: UnitTable::detached(hub_for_table),
        }
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
        // The kernel's word on who this is: a pid the peer cannot forge, and
        // the uid that decides whether it may be the operator.
        let credentials = stream.peer_cred().ok();
        let pid = credentials.as_ref().and_then(|c| c.pid()).unwrap_or(0);
        let uid = credentials.map(|c| c.uid()).unwrap_or(u32::MAX);
        let (mut reader, mut writer) = Transport::new(stream).split();

        let hello = match Self::hello(&mut reader).await {
            Ok(hello) => hello,
            Err(e) => {
                let refusal = Refusal::precondition(e.to_string());
                let _ = writer.send(refusal.frame(0)).await;
                return Err(e);
            }
        };

        let peer = match self.admit(pid, uid, &hello) {
            Ok(peer) => peer,
            Err(refusal) => {
                tracing::warn!(pid, code = ?refusal.code, "connection refused: {}", refusal.message);
                let _ = writer.send(refusal.frame(0)).await;
                return Err(SessionError::Refused(refusal));
            }
        };

        tracing::info!(
            pid,
            unit = %peer.label(),
            manifest_hash = %hello.manifest_hash,
            "handshake complete"
        );

        // What this peer may see, fixed at admission from its manifest.
        let mut subscriptions = self.subscriptions(&peer);

        // Snapshot and subscription are taken atomically, so nothing published
        // in between is lost.
        let (snapshot, mut state) = self.hub.subscribe_state();
        let snapshot = subscriptions.filter_snapshot(snapshot);
        let mut events = self.hub.subscribe_events();

        writer
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
                    config: peer
                        .unit_name()
                        .map(|name| self.units.config(name))
                        .unwrap_or_default(),
                })),
            })
            .await?;

        // A unit is reachable by name for as long as this session lasts.
        let (outbound, mut requests) = tokio::sync::mpsc::channel::<Request>(16);
        let _registered = peer
            .unit_name()
            .map(|name| self.units.connected(name, outbound));

        let mut streams = DaemonStreams::new();
        let mut pending: HashMap<u64, oneshot::Sender<Result<result::Outcome, Refusal>>> =
            HashMap::new();

        let dispatcher = Dispatcher::new(
            self.hub.clone(),
            self.supervisor.clone(),
            self.units.clone(),
        );
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
                                writer.send(Self::patch_frame(patch)).await?;
                            }
                        }
                        // The mirror is now behind by an unknown amount, and a
                        // silently stale mirror is worse than a slow one: send
                        // the whole truth rather than let it drift forever.
                        Err(broadcast::error::RecvError::Lagged(missed)) => {
                            tracing::warn!(unit = %peer.label(), missed, "state subscriber lagged; resyncing");
                            if let Some(patch) = subscriptions.filter(&self.snapshot_patch()) {
                                writer.send(Self::patch_frame(patch)).await?;
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
                            writer.send(Self::event_frame(event)).await?;
                        }
                        Ok(_) => {}
                        Err(broadcast::error::RecvError::Lagged(missed)) => {
                            tracing::warn!(unit = %peer.label(), missed, "event subscriber lagged");
                        }
                        Err(broadcast::error::RecvError::Closed) => return Ok(()),
                    }
                }
                // The daemon asking this unit for something.
                Some(request) = requests.recv() => {
                    let stream_id = streams.allocate();
                    pending.insert(stream_id, request.answer);
                    writer.send(Frame {
                        stream_id,
                        body: Some(frame::Body::Invoke(omega_proto::omega::Invoke {
                            op: Some(request.op),
                        })),
                    }).await?;
                }
                received = reader.recv() => {
                    match received {
                        Ok(Some(frame)) => {
                            liveness.seen();
                            if Self::answers_daemon(&frame, &mut pending) {
                                continue;
                            }
                            self.handle(&dispatcher, &peer, &mut subscriptions, &frame, &mut writer).await?;
                        }
                        Ok(None) => return Ok(()),
                        Err(e) => return Err(SessionError::Transport(e)),
                    }
                }
                _ = self.shutdown.wait() => {
                    tracing::debug!(unit = %peer.label(), "daemon shutting down; closing session");
                    return Ok(());
                }
                _ = keepalive.tick() => {
                    if liveness.health() == Health::Unresponsive {
                        tracing::warn!(unit = %peer.label(), "peer stopped answering; closing");
                        return Err(SessionError::Unresponsive(peer.label()));
                    }
                    writer.send(Self::ping()).await?;
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
    async fn hello(reader: &mut ReadHalf<UnixStream>) -> Result<Hello, SessionError> {
        let frame = tokio::time::timeout(Handshake::TIMEOUT, reader.recv())
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
    fn answers_daemon(
        frame: &Frame,
        pending: &mut HashMap<u64, oneshot::Sender<Result<result::Outcome, Refusal>>>,
    ) -> bool {
        if !DaemonStreams::is_ours(frame.stream_id) {
            return false;
        }
        let Some(answer) = pending.remove(&frame.stream_id) else {
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

        let _ = answer.send(outcome);
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
        writer: &mut WriteHalf<UnixStream>,
    ) -> Result<(), SessionError> {
        let answer = match frame.body.as_ref() {
            Some(frame::Body::Invoke(invoke)) => {
                dispatcher.invoke(peer, subscriptions, invoke).await
            }
            Some(frame::Body::Hello(_)) => Err(Refusal::invalid("Hello after the handshake")),
            // A peer may check on the daemon too.
            Some(frame::Body::Ping(ping)) => {
                writer
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
            Ok(response) => writer.send(response.frame(frame.stream_id)).await?,
            Err(refusal) => {
                tracing::warn!(
                    unit = %peer.label(),
                    code = ?refusal.code,
                    "refused: {}", refusal.message
                );
                writer.send(refusal.frame(frame.stream_id)).await?;
            }
        }
        Ok(())
    }
}
