//! The observation socket: everything the daemon holds, as newline-delimited
//! JSON — published views and state topics as they change — and the one way
//! back for a shell that draws them.
//!
//! Quickshell (QML) reads the views directly with `Socket`; `omega status`
//! reads the `units` topic from the same stream. Reading needs no handshake
//! and is granted to anyone who can open the socket.
//!
//! Asking is different. A shell cannot encode protobuf, so a request is the
//! same [`Frame`] carrying the same `Invoke`, written as JSON, and is
//! authorized the way every other request is: the peer must be the daemon's
//! own user, and the op must be one the [`POLICY`] table serves an operator.
//! A JSON request reaches the same [`Dispatcher`] a framed one does.
//!
//! [`POLICY`]: crate::session::dispatch
//! [`Frame`]: omega_proto::omega::Frame

use crate::session::operations::Operations;
use std::path::Path;
use std::sync::Arc;

use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};
use tokio::task::JoinSet;

mod requests;
#[cfg(test)]
mod tests;
use requests::Requests;
use tokio::net::UnixStream;
use tokio::sync::broadcast;

use omega_proto::omega::{Frame, Invoke, frame};
use omega_proto::{Observation, Refusal, Socket};

use crate::broker::Brokerage;
use crate::hub::{Heartbeat, Hub, ViewUpdate};
use crate::session::admission::Peer;
use crate::session::{Dispatcher, Subscriptions};
use crate::supervisor::Supervisor;
use crate::units::UnitTable;
use std::path::PathBuf;

/// What a request needs to be served, once a peer has been admitted.
#[derive(Debug, Clone)]
struct Gateway {
    supervisor: Supervisor,
    units: UnitTable,
    brokers: Brokerage,
    layout: Option<omega_host::Layout>,
}

/// Streams each surface's latest view as a JSON line over a dedicated socket,
/// and serves what its readers ask of it.
#[derive(Debug)]
pub struct ShellServer {
    hub: Hub,
    listener: omega_proto::BoundSocket,
    socket: Socket,
    /// Absent for a server that only streams — a test watching views, and
    /// nothing a person runs.
    gateway: Option<Gateway>,
}

impl ShellServer {
    pub fn bind(hub: Hub) -> Result<Self, ShellError> {
        Self::bind_at(Observation::socket(), hub)
    }

    /// Bind an explicit socket (tests, unusual deployments).
    pub fn bind_at(socket: Socket, hub: Hub) -> Result<Self, ShellError> {
        let listener = socket.bind().map_err(|source| ShellError::Bind {
            path: socket.path().to_path_buf(),
            source,
        })?;
        Ok(Self {
            hub,
            listener,
            socket,
            gateway: None,
        })
    }

    /// Serve requests as well as stream: a shell that draws a button can
    /// press it.
    pub fn serving(mut self, supervisor: Supervisor, units: UnitTable, brokers: Brokerage) -> Self {
        self.gateway = Some(Gateway {
            supervisor,
            units,
            brokers,
            layout: None,
        });
        self
    }

    pub fn with_layout(mut self, layout: omega_host::Layout) -> Self {
        if let Some(gateway) = &mut self.gateway {
            gateway.layout = Some(layout);
        }
        self
    }

    pub fn path(&self) -> &Path {
        self.socket.path()
    }

    /// Accept observers and stream to each. Dropping this future cancels its connections.
    pub async fn run(&self) -> Result<(), ShellError> {
        let mut connections = JoinSet::new();
        let result = self.accepting(&mut connections).await;
        connections.shutdown().await;
        result
    }

    pub(crate) const CONNECTION_LIMIT: usize = 64;

    /// The caller owns tasks across cancellation of the listener future.
    pub(crate) async fn accepting(&self, connections: &mut JoinSet<()>) -> Result<(), ShellError> {
        loop {
            tokio::select! {
                Some(result) = connections.join_next(), if !connections.is_empty() => {
                    if let Err(error) = result { tracing::warn!(%error, "observer task failed"); }
                }
                accepted = self.listener.accept(), if connections.len() < Self::CONNECTION_LIMIT => {
                    let (stream, _) = accepted?;
                    let (views, view_rx) = self.hub.subscribe_views();
                    let (state, state_rx) = self.hub.subscribe_state();
                    let hub = self.hub.clone();
                    let peer = Self::admit(&stream);
                    let gateway = self.gateway.clone();
                    connections.spawn(async move {
                        let connection = ShellConnection::new(stream, view_rx, state_rx, hub, peer, gateway);
                        if let Err(error) = connection.stream(views, state).await {
                            tracing::debug!(%error, "observer connection ended");
                        }
                    });
                }
            }
        }
    }
}

/// Who is on the other end, if they are anyone the daemon serves.
///
/// The same question the control socket asks, answered the same way: the uid
/// is the whole claim, because on a Unix socket there is nothing stronger to
/// ask for.
impl ShellServer {
    fn admit(stream: &UnixStream) -> Result<Peer, Refusal> {
        let credentials = stream.peer_cred().ok();
        let pid = credentials.as_ref().and_then(|c| c.pid()).unwrap_or(0);
        let uid = credentials.map(|c| c.uid()).unwrap_or(u32::MAX);
        Peer::operator(pid, uid)
    }
}

/// One connected observer: everything current, then everything that changes —
/// and whatever it asks for meanwhile.
struct ShellConnection<S> {
    writer: tokio::io::WriteHalf<S>,
    requests: Requests<tokio::io::ReadHalf<S>>,
    views: crate::hub::history::Receiver<Arc<ViewUpdate>>,
    state: crate::hub::history::Receiver<omega_proto::omega::StatePatch>,
    hub: Hub,
    /// Who this is, or why they are nobody. Reading does not depend on it;
    /// asking does.
    peer: Result<Arc<Peer>, Refusal>,
    gateway: Option<Gateway>,
    sent_views: std::collections::BTreeMap<crate::hub::SurfaceRef, u64>,
}

impl<S: AsyncRead + AsyncWrite + Unpin> ShellConnection<S> {
    /// How often a connection says it is still there when nothing else has.
    ///
    /// Comfortably inside the silence an observer treats as a dead daemon —
    /// the shell reconnects after fifteen seconds of nothing — so a quiet
    /// machine never looks like a stopped one.
    const HEARTBEAT: std::time::Duration = std::time::Duration::from_secs(5);

    fn new(
        stream: S,
        views: crate::hub::history::Receiver<Arc<ViewUpdate>>,
        state: crate::hub::history::Receiver<omega_proto::omega::StatePatch>,
        hub: Hub,
        peer: Result<Peer, Refusal>,
        gateway: Option<Gateway>,
    ) -> Self {
        let (reader, writer) = tokio::io::split(stream);
        Self {
            writer,
            requests: Requests::new(reader),
            sent_views: Default::default(),
            views,
            state,
            hub,
            peer: peer.map(Arc::new),
            gateway,
        }
    }

    /// The current picture first, so an observer that connects late is not
    /// waiting on the next change to learn anything; then live updates, and
    /// answers to anything asked along the way.
    async fn stream(
        mut self,
        views: Vec<Arc<ViewUpdate>>,
        state: omega_proto::omega::StateSnapshot,
    ) -> Result<(), ShellError> {
        // Everything, until the observer says otherwise: reading this socket
        // needs no handshake, so a peer that asks for nothing is a peer that
        // wants what the daemon holds.
        let mut subscriptions = Subscriptions::watcher();

        self.write_views(views).await?;
        self.write_topics(&subscriptions, &state.topics).await?;
        drop(state);

        // One dispatcher per connection, as a session has: what it holds on
        // this peer's behalf is given back when the connection ends.
        let dispatcher = self.gateway.as_ref().map(|gateway| {
            Dispatcher::new(
                self.hub.clone(),
                gateway.supervisor.clone(),
                gateway.units.clone(),
                gateway.brokers.clone(),
            )
            .with_layout(gateway.layout.clone())
        });
        let dispatcher = dispatcher.map(Arc::new);
        let mut operations = Operations::new();
        let mut beat = tokio::time::interval(Self::HEARTBEAT);
        beat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        // The first tick is immediate and would beat before anything was
        // said; the connection has just written everything it holds.
        beat.tick().await;

        loop {
            tokio::select! {
                view = self.views.recv() => match view {
                    Ok(view) => self.write_views([view]).await?,
                    // Dropped views would leave surfaces frozen at a stale
                    // tree with nothing to correct them, so resend them all.
                    Err(broadcast::error::RecvError::Lagged(missed)) => {
                        tracing::warn!(missed, "observer lagged on views; resending every surface");
                        self.resync_views().await?;
                    }
                    Err(broadcast::error::RecvError::Closed) => return Ok(()),
                },
                patch = self.state.recv() => match patch {
                    Ok(patch) => self.write_topics(&subscriptions, &patch.topics).await?,
                    Err(broadcast::error::RecvError::Lagged(missed)) => {
                        tracing::warn!(missed, "observer lagged on state; resending every topic");
                        let (snapshot, receiver) = self.hub.subscribe_state();
                        self.state = receiver;
                        self.write_topics(&subscriptions, &snapshot.topics).await?;
                    }
                    Err(broadcast::error::RecvError::Closed) => return Ok(()),
                },
                line = self.requests.next_line() => match line? {
                    Some(line) if line.trim().is_empty() => {}
                    Some(line) => {
                        let frame: Frame = match serde_json::from_str(&line) {
                            Ok(frame) => frame,
                            Err(error) => {
                                self.write_answer(&Refusal::invalid(format!("not a request: {error}")).frame(0)).await?;
                                continue;
                            }
                        };
                        if let (Some(dispatcher), Ok(peer), Some(frame::Body::Invoke(invoke))) =
                            (&dispatcher, &self.peer, &frame.body)
                            && Operations::deferred(invoke)
                        {
                            if let Err(refusal) = operations.start(
                                dispatcher.clone(), peer.clone(), subscriptions.clone(),
                                frame.stream_id, invoke.clone(),
                            ) {
                                self.write_answer(&refusal.frame(frame.stream_id)).await?;
                            }
                        } else {
                            let answer = self.answer(dispatcher.as_deref(), &mut subscriptions, frame).await;
                            self.write_answer(&answer).await?;
                        }
                    }
                    // The observer hung up. Its views have nowhere to go.
                    None => return Ok(()),
                },
                result = operations.next(), if !operations.is_empty() => {
                    let answer = result.map_err(|error| std::io::Error::other(error.to_string()))?;
                    self.write_answer(&answer).await?;
                },
                _ = beat.tick() => self.write_line(&Heartbeat { heartbeat: true }).await?,
            }
        }
    }

    /// Serve one request, or say why not. Every request is answered on the
    /// stream that carried it, exactly as on the control socket: an outcome
    /// or a refusal, never silence.
    async fn answer(
        &self,
        dispatcher: Option<&Dispatcher>,
        subscriptions: &mut Subscriptions,
        frame: Frame,
    ) -> Frame {
        let stream_id = frame.stream_id;

        let Some(frame::Body::Invoke(Invoke { op: Some(op) })) = frame.body else {
            return Refusal::invalid("a request carries an invoke").frame(stream_id);
        };

        let Some(dispatcher) = dispatcher else {
            return Refusal::unimplemented("this daemon serves no requests here").frame(stream_id);
        };

        // Reading is open; asking is the owner's. A peer that was turned away
        // at accept is told so here rather than at a silent drop.
        let peer = match &self.peer {
            Ok(peer) => peer,
            Err(refusal) => return refusal.clone().frame(stream_id),
        };

        match dispatcher
            .invoke(peer, subscriptions, &Invoke { op: Some(op) })
            .await
        {
            Ok(response) => response.frame(stream_id),
            Err(refusal) => {
                tracing::warn!(code = ?refusal.code, "shell request refused: {}", refusal.message);
                refusal.frame(stream_id)
            }
        }
    }

    async fn resync_views(&mut self) -> Result<(), ShellError> {
        let (snapshot, receiver) = self.hub.subscribe_views();
        self.views = receiver;
        let current: std::collections::BTreeSet<_> =
            snapshot.iter().map(|view| &view.surface).collect();
        let removed: Vec<_> = self
            .sent_views
            .iter()
            .filter(|(surface, _)| !current.contains(surface))
            .map(|(surface, revision)| {
                Arc::new(ViewUpdate {
                    surface: surface.clone(),
                    view: omega_proto::omega::ViewTree {
                        root: None,
                        revision: revision.checked_add(1).expect("view revision exhausted"),
                    },
                })
            })
            .collect();
        self.write_views(removed).await?;
        self.write_views(snapshot).await
    }

    async fn write_views(
        &mut self,
        views: impl IntoIterator<Item = Arc<ViewUpdate>>,
    ) -> Result<(), ShellError> {
        tokio::time::timeout(Self::WRITE_TIMEOUT, async {
            for view in views {
                self.write_line(view.as_ref()).await?;
                if view.view.root.is_some() {
                    self.sent_views
                        .insert(view.surface.clone(), view.view.revision);
                } else {
                    self.sent_views.remove(&view.surface);
                }
            }
            Ok(())
        })
        .await
        .map_err(|_| ShellError::SnapshotTimeout)?
    }

    /// The topics this observer asked for, one line each.
    ///
    /// Filtered by the same [`Subscriptions`] a unit's session filters by. It
    /// was not filtered at all: the shell draws views and was sent every topic
    /// the daemon held, and so was every other reader of this socket.
    async fn write_topics(
        &mut self,
        subscriptions: &Subscriptions,
        topics: &[omega_proto::omega::StateTopic],
    ) -> Result<(), ShellError> {
        tokio::time::timeout(Self::WRITE_TIMEOUT, async {
            for topic in topics {
                if subscriptions.wants(&topic.topic) {
                    self.write_line(topic).await?;
                }
            }
            Ok(())
        })
        .await
        .map_err(|_| ShellError::SnapshotTimeout)?
    }

    async fn write_line(&mut self, observed: &impl serde::Serialize) -> Result<(), ShellError> {
        self.write(serde_json::to_string(observed)?).await
    }

    async fn write_answer(&mut self, answer: &Frame) -> Result<(), ShellError> {
        self.write(Observation::line(answer)?).await
    }

    const WRITE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

    async fn write(&mut self, line: String) -> Result<(), ShellError> {
        let writing = tokio::time::timeout(Self::WRITE_TIMEOUT, async {
            self.writer.write_all(line.as_bytes()).await?;
            self.writer.write_all(b"\n").await?;
            self.writer.flush().await
        });
        tokio::pin!(writing);
        loop {
            tokio::select! {
                biased;
                result = &mut writing => {
                    result.map_err(|_| ShellError::WriteTimeout)??;
                    return Ok(());
                }
                result = self.requests.buffer_next(), if !self.requests.closed => { result?; }
            }
        }
    }
}

/// What serving the shell socket can fail with.
#[derive(Debug, thiserror::Error)]
pub enum ShellError {
    #[error("observer snapshot batch deadline elapsed")]
    SnapshotTimeout,
    #[error("observer write deadline elapsed")]
    WriteTimeout,
    #[error("cannot bind shell socket {}: {source}", path.display())]
    Bind {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("cannot serialize view: {0}")]
    Encode(#[from] serde_json::Error),
}
