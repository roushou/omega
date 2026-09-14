//! Route subsystem actions to broker-owned driver tasks.
//! Connections are accessed only by their driver; requests enter through bounded queues.

mod driver;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use std::time::Duration;
use tokio::sync::{OwnedSemaphorePermit, Semaphore, mpsc, oneshot};
use tokio::task::JoinHandle;
use tokio::time::{Instant, timeout_at};

use omega_platform::{Broker, BrokerError};
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

/// Shared broker handles and action routing for daemon sessions.
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
    /// Maximum queued actions per broker before admission is refused.
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

    /// Start a driver and register its declared action routes.
    /// Brokers with no actions receive a closed inbox that remains pending.
    pub fn add(&self, broker: Box<dyn Broker>) {
        let (requests, inbox) = mpsc::channel(Self::QUEUE);
        {
            let mut routes = self.inner.routes.lock().unwrap_or_else(|e| e.into_inner());
            for kind in broker.actions() {
                // Each action kind must have exactly one broker; coverage tests enforce uniqueness.
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

    /// Dispatch to the broker for this action kind. `None` means no broker claims it.
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

    /// Wait for broker drivers to observe shutdown and drop pending operations.
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
