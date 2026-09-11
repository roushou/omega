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

use std::time::Duration;
use tokio::sync::{OwnedSemaphorePermit, Semaphore, mpsc, oneshot};
use tokio::task::JoinHandle;
use tokio::time::{Instant, timeout_at};

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
    deadline: Instant,
    _bytes: OwnedSemaphorePermit,
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
    bytes: Arc<Semaphore>,
    running: Mutex<Vec<JoinHandle<()>>>,
    /// Which broker serves which kind. One sender per broker, cloned per
    /// kind it claimed.
    routes: Mutex<HashMap<ActionKind, mpsc::Sender<Request>>>,
}

impl Brokerage {
    /// How many actions may be queued for one broker before admission fails.
    /// Small on purpose: a broker that cannot keep up should make the caller
    /// feel it rather than build a backlog of stale brightness steps.
    const QUEUE: usize = 8;
    const BYTE_LIMIT: usize = 8 * 1024 * 1024;
    const ACTION_TIMEOUT: Duration = Duration::from_secs(5);

    pub fn new(hub: Hub, shutdown: Shutdown) -> Self {
        Self {
            inner: Arc::new(Inner {
                hub,
                shutdown,
                bytes: Arc::new(Semaphore::new(Self::BYTE_LIMIT)),
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

        let task_name = format!("broker {}", broker.name());
        let shutdown = self.inner.shutdown.clone();
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
            .push(tokio::spawn(async move {
                shutdown.supervise(task_name, driver.run()).await
            }));
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

        let size = action.encoded_len();
        if size > omega_proto::MAX_FRAME_LEN {
            return Some(Err(BrokerError::TooLarge));
        }
        let bytes = match self.inner.bytes.clone().try_acquire_many_owned(size as u32) {
            Ok(bytes) => bytes,
            Err(_) => return Some(Err(BrokerError::Full)),
        };
        let slot = match route.try_reserve() {
            Ok(slot) => slot,
            Err(mpsc::error::TrySendError::Full(_)) => return Some(Err(BrokerError::Full)),
            Err(mpsc::error::TrySendError::Closed(_)) => return Some(Err(BrokerError::gone())),
        };
        let deadline = Instant::now() + Self::ACTION_TIMEOUT;
        let (answer, answered) = oneshot::channel();
        slot.send(Request {
            action: action.clone(),
            answer,
            deadline,
            _bytes: bytes,
        });
        Some(match timeout_at(deadline, answered).await {
            Ok(answer) => answer.unwrap_or_else(|_| Err(BrokerError::gone())),
            Err(_) => Err(BrokerError::Timeout),
        })
    }

    /// Wait for every broker to notice the shutdown and stop.
    ///
    /// They are asked by the same `Shutdown` everything else selects on, so
    /// the driver drops an outstanding operation before releasing its broker.
    pub async fn stop(&self) {
        let handles: Vec<_> =
            std::mem::take(&mut *self.inner.running.lock().unwrap_or_else(|e| e.into_inner()));
        for handle in handles {
            if let Err(error) = handle.await {
                tracing::error!(%error, "broker task join failed");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture;
    impl Fixture {
        fn queue() -> (Brokerage, mpsc::Receiver<Request>) {
            let brokers = Brokerage::new(Hub::new(), Shutdown::new());
            let (sender, receiver) = mpsc::channel(Brokerage::QUEUE);
            brokers
                .inner
                .routes
                .lock()
                .unwrap()
                .insert(ActionKind::RunCommand, sender);
            (brokers, receiver)
        }
        fn action(size: usize) -> action::Kind {
            action::Kind::RunCommand(omega_proto::omega::RunCommand {
                command: "x".repeat(size),
            })
        }
    }

    #[tokio::test(start_paused = true)]
    async fn admission_bounds_bytes_before_cloning_and_releases_them_with_requests() {
        let (brokers, mut receiver) = Fixture::queue();
        let action = Fixture::action(3 * 1024 * 1024);
        let first = brokers.act(&action);
        let second = brokers.act(&action);
        tokio::pin!(first, second);
        let (one, two) = tokio::select! {
            biased;
            _ = &mut first => panic!("first was not queued"),
            _ = &mut second => panic!("second was not queued"),
            requests = async { (receiver.recv().await.unwrap(), receiver.recv().await.unwrap()) } => requests,
        };
        assert!(matches!(
            brokers.act(&action).await,
            Some(Err(BrokerError::Full))
        ));
        drop(one);
        drop(two);
        assert_eq!(
            brokers.inner.bytes.available_permits(),
            Brokerage::BYTE_LIMIT
        );
        assert!(matches!(
            brokers
                .act(&Fixture::action(omega_proto::MAX_FRAME_LEN))
                .await,
            Some(Err(BrokerError::TooLarge))
        ));
    }

    #[tokio::test(start_paused = true)]
    async fn queue_wait_has_a_deadline_and_a_closed_caller_cannot_execute_later() {
        let (brokers, mut receiver) = Fixture::queue();
        assert!(matches!(
            brokers.act(&Fixture::action(1)).await,
            Some(Err(BrokerError::Timeout))
        ));
        let request = receiver.recv().await.unwrap();
        assert!(request.answer.is_closed());
        assert!(request.deadline <= Instant::now());
        drop(request);
        assert_eq!(
            brokers.inner.bytes.available_permits(),
            Brokerage::BYTE_LIMIT
        );
    }

    #[tokio::test]
    async fn count_saturation_refuses_without_waiting_or_leaking_bytes() {
        let (brokers, mut receiver) = Fixture::queue();
        let route = brokers.inner.routes.lock().unwrap()[&ActionKind::RunCommand].clone();
        let mut slots = Vec::new();
        for _ in 0..Brokerage::QUEUE {
            slots.push(route.try_reserve().unwrap());
        }
        assert!(matches!(
            brokers.act(&Fixture::action(1)).await,
            Some(Err(BrokerError::Full))
        ));
        assert_eq!(
            brokers.inner.bytes.available_permits(),
            Brokerage::BYTE_LIMIT
        );
        assert!(receiver.try_recv().is_err());
    }
}
