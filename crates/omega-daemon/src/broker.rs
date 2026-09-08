//! Running brokers, and routing actions to them.
//!
//! A broker reports changes and serves actions; this is the half that decides
//! when to ask, what to do with a failure, and when to stop. Cadence belongs
//! to the broker — it knows whether its subsystem has signals — and
//! everything else is the same for all of them, which is why there is one
//! driver rather than a loop per subsystem.
//!
//! A broker owns its connection, so nothing else may touch it: an action is a
//! message to the task that holds it, not a lock taken around it. That is
//! what keeps a broker blocked on a thirty-second signal from blocking the
//! volume key.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;

use omega_brokers::{Broker, BrokerError};
use omega_proto::ActionKind;
use omega_proto::omega::action;

use crate::hub::Hub;
use crate::shutdown::Shutdown;
use crate::supervisor::Backoff;

/// One action, and somewhere to put the answer.
#[derive(Debug)]
struct Request {
    action: action::Kind,
    answer: oneshot::Sender<Result<(), BrokerError>>,
}

/// The brokers a daemon is running.
///
/// A cloneable handle, like [`Hub`] and the unit table: the dispatcher needs
/// to reach it per connection, and what it holds is shared by construction.
#[derive(Debug, Clone)]
pub struct Brokerage {
    inner: Arc<Inner>,
}

#[derive(Debug)]
struct Inner {
    hub: Hub,
    shutdown: Shutdown,
    running: Mutex<Vec<JoinHandle<()>>>,
    /// Which broker serves which kind. One sender per broker, cloned per
    /// kind it claimed.
    routes: Mutex<HashMap<ActionKind, mpsc::Sender<Request>>>,
}

impl Brokerage {
    /// How many actions may be queued for one broker before a caller waits.
    /// Small on purpose: a broker that cannot keep up should make the caller
    /// feel it rather than build a backlog of stale brightness steps.
    const QUEUE: usize = 8;

    pub fn new(hub: Hub, shutdown: Shutdown) -> Self {
        Self {
            inner: Arc::new(Inner {
                hub,
                shutdown,
                running: Mutex::new(Vec::new()),
                routes: Mutex::new(HashMap::new()),
            }),
        }
    }

    /// Start a broker: its topics are published into the hub, and the kinds
    /// it declared are routed to it.
    pub fn add(&self, broker: Box<dyn Broker>) {
        let (requests, inbox) = mpsc::channel(Self::QUEUE);
        {
            let mut routes = self.inner.routes.lock().unwrap_or_else(|e| e.into_inner());
            for kind in broker.actions() {
                // Two brokers claiming one kind would make the route depend
                // on registration order, which is not a thing to debug at
                // three in the morning. `omega-brokers` has a test.
                debug_assert!(
                    !routes.contains_key(kind),
                    "{} is claimed by more than one broker",
                    kind.name()
                );
                routes.insert(*kind, requests.clone());
            }
        }

        let handle = tokio::spawn(Self::drive(
            broker,
            inbox,
            self.inner.hub.clone(),
            self.inner.shutdown.clone(),
        ));
        self.inner
            .running
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(handle);
    }

    /// Ask whichever broker serves this kind to perform it.
    ///
    /// `None` means no broker claims it, which is not the same as a broker
    /// refusing: the daemon answers the two differently, so a capability is
    /// never granted for something nothing can do.
    pub async fn act(&self, action: &action::Kind) -> Option<Result<(), BrokerError>> {
        let route = self
            .inner
            .routes
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&ActionKind::of(action))
            .cloned()?;

        let (answer, answered) = oneshot::channel();
        let request = Request {
            action: action.clone(),
            answer,
        };
        if route.send(request).await.is_err() {
            return Some(Err(BrokerError::Unreadable {
                subsystem: "broker",
                detail: "the broker that serves this stopped".into(),
            }));
        }

        Some(answered.await.unwrap_or_else(|_| {
            Err(BrokerError::Unreadable {
                subsystem: "broker",
                detail: "the broker answered nothing".into(),
            })
        }))
    }

    /// Wait for every broker to notice the shutdown and stop.
    ///
    /// They are asked by the same `Shutdown` everything else selects on, so
    /// this only waits — it does not abort. A broker mid-read finishes it.
    pub async fn stop(&self) {
        let handles: Vec<_> =
            std::mem::take(&mut *self.inner.running.lock().unwrap_or_else(|e| e.into_inner()));
        for handle in handles {
            let _ = handle.await;
        }
    }

    /// One broker, until the daemon stops.
    ///
    /// A failure is not fatal. A subsystem that goes away comes back — the
    /// user restarted PipeWire, the adapter was plugged in again — so the
    /// broker is asked again after a backoff rather than abandoned. The
    /// backoff resets on the first reading that works, so an outage an hour
    /// ago does not slow down the reading taken now.
    async fn drive(
        mut broker: Box<dyn Broker>,
        mut inbox: mpsc::Receiver<Request>,
        hub: Hub,
        shutdown: Shutdown,
    ) {
        let name = broker.name();
        let mut backoff = Backoff::new();

        loop {
            // Taken inside the select and dropped before it is used, because
            // `next` and `act` are both `&mut self` and only one may be
            // borrowed at a time. An arriving action cancels the pending
            // `next`, which is why the trait requires it to be cancel-safe.
            let request = tokio::select! {
                // Shutdown first, so stopping does not depend on which of
                // several ready branches the runtime picks.
                biased;
                _ = shutdown.wait() => break,
                // `None` is every sender gone, which cannot happen while
                // the brokerage holds the route. Keep reporting either way.
                request = inbox.recv() => request,
                change = broker.next() => {
                    match change {
                        Ok(patch) => {
                            backoff.reset();
                            hub.publish_state(patch);
                        }
                        Err(error) => {
                            let delay = backoff.delay();
                            tracing::error!(
                                broker = name,
                                %error,
                                ?delay,
                                attempt = backoff.attempts(),
                                "broker failed"
                            );
                            tokio::select! {
                                biased;
                                _ = shutdown.wait() => break,
                                _ = tokio::time::sleep(delay) => {}
                            }
                        }
                    }
                    None
                }
            };

            if let Some(request) = request {
                let outcome = broker.act(&request.action).await;
                if let Ok(Some(patch)) = &outcome {
                    // What the action changed, without waiting for the poll
                    // to notice it.
                    hub.publish_state(patch.clone());
                }
                if let Err(error) = &outcome {
                    tracing::warn!(
                        broker = name,
                        action = ActionKind::of(&request.action).name(),
                        %error,
                        "broker refused an action"
                    );
                }
                // The caller may have given up; that is not this broker's
                // problem.
                let _ = request.answer.send(outcome.map(|_| ()));
            }
        }

        tracing::debug!(broker = name, "broker stopped");
    }
}
