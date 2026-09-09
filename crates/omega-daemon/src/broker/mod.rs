//! Running brokers, and routing actions to them.
//!
//! A broker reports changes and serves actions. This is the half that decides
//! which broker serves what and hands each one to a [`Driver`]; when to ask,
//! and what to do with a failure, is the driver's.
//!
//! A broker owns its connection, so nothing else may touch it: an action is a
//! message to the task that holds it, not a lock taken around it. That is
//! what keeps a broker blocked on a thirty-second signal from blocking the
//! volume key.

mod driver;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;

use omega_brokers::{Broker, BrokerError};
use omega_proto::ActionKind;
use omega_proto::omega::action;

use crate::hub::Hub;
use crate::shutdown::Shutdown;
use driver::Driver;

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
    ///
    /// A broker that claims no kinds is routed nothing, so no sender outlives
    /// this call and its inbox is closed before it takes its first reading.
    /// That is a broker with nothing to serve, not a broker with nothing to
    /// do, and holding those apart is the driver's inbox's whole job.
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

        let driver = Driver::new(
            broker,
            inbox,
            self.inner.hub.clone(),
            self.inner.shutdown.clone(),
        );
        self.inner
            .running
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(tokio::spawn(driver.run()));
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
            return Some(Err(BrokerError::unreadable(
                "the broker that serves this stopped",
            )));
        }

        Some(
            answered
                .await
                .unwrap_or_else(|_| Err(BrokerError::unreadable("the broker answered nothing"))),
        )
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
}
