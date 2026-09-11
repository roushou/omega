//! Running one broker.
//!
//! The rules about holding a connection, written once rather than once per
//! subsystem: open lazily and again after it goes; take the first reading at
//! once; count a broker reporting only *after* a reading succeeds, so a
//! cancelled wait asks again rather than waiting on a change whose state it
//! already missed; drop the connection when what was being waited on closes.
//!
//! A failure is not fatal. A subsystem that goes away comes back — the user
//! restarted PipeWire, the adapter was plugged in again — so the broker is
//! asked again after a backoff rather than abandoned. The backoff resets on
//! the first reading that works, so an outage an hour ago does not slow down
//! the reading taken now.
//!
//! One thing stops a driver, and it is [`Shutdown`]. Everything else it waits
//! on has to be able to mean "nothing, ever" without saying so by resolving:
//! [`Broker::wake`] does it with `pending`, and [`Inbox`] is what makes the
//! other arm of the same select obey the same rule.

use std::fmt;
use std::ops::ControlFlow;
use std::time::Duration;
use tokio::time::{Instant, timeout, timeout_at};

use tokio::sync::mpsc;

use omega_brokers::{Broker, BrokerError};
use omega_proto::ActionKind;

use super::Request;
use crate::hub::Hub;
use crate::shutdown::Shutdown;
use crate::supervisor::Backoff;

/// The actions arriving for one broker.
///
/// It exists so that "no action will ever arrive" is not an answer. A
/// `Receiver` whose senders are gone says it by resolving to `None` on every
/// poll, and in a `select!` an arm that is always ready is not silence — it
/// starves the arms beside it. [`Broker::wake`] already holds this rule on
/// the other arm of the same select, where nothing to wait for is `pending`
/// rather than a value; this holds it here.
struct Inbox(mpsc::Receiver<Request>);

impl Inbox {
    /// The next action to serve.
    ///
    /// Never resolves once the last sender is gone. A closed channel means
    /// the brokerage routes nothing here — a broker that claimed no kinds, or
    /// one outliving the brokerage — and neither is a reason to read or to
    /// stop.
    ///
    /// Cancel-safe: `recv` is, and a closed channel stays closed, so a
    /// dropped future leaves nothing behind.
    async fn next(&mut self) -> Request {
        match self.0.recv().await {
            Some(request) => request,
            None => std::future::pending().await,
        }
    }
}

/// What the driver has with its broker.
///
/// The three states the connection rules describe. They were two booleans,
/// `open` and `primed`, whose fourth combination — reporting with nothing
/// open — meant nothing and was reachable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Contact {
    /// Nothing open. Connect before anything else.
    Closed,
    /// Open, with nothing read yet. The first reading is taken at once rather
    /// than waited for, or a bar stays blank until something moves.
    Opened,
    /// Open and reporting. The next reading waits for the broker to wake.
    Primed,
}

impl Contact {
    /// Wait until a reading is due.
    ///
    /// Cancel-safe, because [`Broker::wake`] is required to be: the driver
    /// selects this against shutdown and the inbox, so the future is dropped
    /// and remade around anything else that happens.
    async fn due(self, broker: &mut dyn Broker) -> Result<(), BrokerError> {
        match self {
            Self::Primed => broker.wake().await,
            Self::Opened => Ok(()),
            // Unreachable: `Driver::step` connects before it selects. Pending
            // rather than a panic, because the only wrong answer here is a
            // ready one — the same rule `Inbox` exists to hold.
            Self::Closed => std::future::pending().await,
        }
    }
}

/// What one turn of the driver does.
///
/// Closed, so no branch of the select can mean two things. It was
/// `Option<Request>`, where `None` meant both "the broker woke, take a
/// reading" and "no action can ever arrive" — and the second was reached on
/// every poll by every broker that claims no actions.
#[derive(Debug)]
enum Turn {
    /// The daemon is stopping.
    Stop,
    /// An action to serve.
    Serve(Request),
    /// A reading is due.
    Read,
    /// What was being waited on is gone; reopen before reading again.
    Reopen(BrokerError),
}

/// One broker, until the daemon stops.
pub(super) struct Driver {
    broker: Box<dyn Broker>,
    inbox: Inbox,
    hub: Hub,
    shutdown: Shutdown,
    backoff: Backoff,
    contact: Contact,
}

impl Driver {
    const IO_TIMEOUT: Duration = Duration::from_secs(10);
    pub(super) fn new(
        broker: Box<dyn Broker>,
        requests: mpsc::Receiver<Request>,
        hub: Hub,
        shutdown: Shutdown,
    ) -> Self {
        Self {
            broker,
            inbox: Inbox(requests),
            hub,
            shutdown,
            backoff: Backoff::new(),
            contact: Contact::Closed,
        }
    }

    /// Drive the broker until the daemon stops.
    pub(super) async fn run(mut self) {
        let shutdown = self.shutdown.clone();
        loop {
            tokio::select! {
                biased;
                () = shutdown.wait() => break,
                progress = self.step() => if progress.is_break() { break; },
            }
        }
        self.broker.disconnect();
        tracing::debug!(broker = self.broker.name(), "broker stopped");
    }

    /// One transition. `Break` when the daemon is stopping.
    ///
    /// Every state change the driver makes is one of these, so a state it can
    /// reach is one this reads.
    async fn step(&mut self) -> ControlFlow<()> {
        if self.contact == Contact::Closed {
            return self.open().await;
        }

        match self.turn().await {
            Turn::Stop => ControlFlow::Break(()),
            Turn::Serve(request) => self.serve(request).await,
            Turn::Read => self.read().await,
            Turn::Reopen(error) => self.pause(error).await,
        }
    }

    /// Wait for whichever comes first: the daemon stopping, an action to
    /// serve, or a reading falling due.
    async fn turn(&mut self) -> Turn {
        // Destructured so the wait can borrow the broker while the inbox is
        // borrowed beside it. Whichever arm loses is dropped, which is why
        // both of them have to be cancel-safe.
        let Self {
            broker,
            inbox,
            shutdown,
            contact,
            ..
        } = self;
        let contact = *contact;

        tokio::select! {
            () = shutdown.wait() => Turn::Stop,
            request = inbox.next() => Turn::Serve(request),
            due = contact.due(broker.as_mut()) => match due {
                Ok(()) => Turn::Read,
                Err(error) => Turn::Reopen(error),
            },
        }
    }

    /// Open whatever the broker holds.
    async fn open(&mut self) -> ControlFlow<()> {
        match timeout(Self::IO_TIMEOUT, self.broker.connect())
            .await
            .unwrap_or(Err(BrokerError::Timeout))
        {
            Ok(()) => {
                self.contact = Contact::Opened;
                ControlFlow::Continue(())
            }
            Err(error) => self.pause(error).await,
        }
    }

    /// Take a reading and publish what it changed.
    async fn read(&mut self) -> ControlFlow<()> {
        match timeout(Self::IO_TIMEOUT, self.broker.read())
            .await
            .unwrap_or(Err(BrokerError::Timeout))
        {
            Ok(patch) => {
                self.backoff.reset();
                // Only once a reading has worked: a read that failed or was
                // cancelled leaves the broker asking again rather than
                // waiting on a change it has already missed the state of.
                self.contact = Contact::Primed;
                if let Err(error) = self.hub.publish_state(patch) {
                    return self.pause(BrokerError::unreadable(error)).await;
                }
                ControlFlow::Continue(())
            }
            Err(error) => self.pause(error).await,
        }
    }

    /// One action, and the answer to whoever asked.
    async fn serve(&mut self, request: Request) -> ControlFlow<()> {
        if request.answer.is_closed() {
            return ControlFlow::Continue(());
        }
        if request.deadline <= Instant::now() {
            let _ = request.answer.send(Err(BrokerError::Timeout));
            return ControlFlow::Continue(());
        }
        let mut outcome = timeout_at(request.deadline, self.broker.act(&request.action))
            .await
            .unwrap_or(Err(BrokerError::Timeout));

        if let Ok(Some(patch)) = &outcome {
            // What the action changed, without waiting for the next wake to
            // notice it.
            if let Err(error) = self.hub.publish_state(patch.clone()) {
                tracing::error!(%error, "broker action changed state that could not be retained");
                outcome = Err(match error {
                    crate::hub::PublishError::TooLarge
                    | crate::hub::PublishError::State(crate::state::StateError::TooLarge) => {
                        BrokerError::TooLarge
                    }
                    _ => BrokerError::Full,
                });
            }
        }
        if let Err(error) = &outcome {
            tracing::warn!(
                broker = self.broker.name(),
                action = ActionKind::of(&request.action).name(),
                %error,
                "broker refused an action"
            );
        }
        // The caller may have given up; that is not this broker's problem.
        let reconnect = matches!(
            &outcome,
            Err(BrokerError::Timeout
                | BrokerError::Io(_)
                | BrokerError::Unreadable(_)
                | BrokerError::Full
                | BrokerError::TooLarge)
        );
        match outcome {
            Err(error) if reconnect => {
                // Reply before backoff, but reset even when the caller has left.
                let message = error.to_string();
                let _ = request.answer.send(Err(error));
                self.pause(BrokerError::unreadable(message)).await
            }
            outcome => {
                let _ = request.answer.send(outcome.map(|_| ()));
                ControlFlow::Continue(())
            }
        }
    }

    /// Wait out a failure, with the connection closed behind it.
    ///
    /// Every failure lands here, so "a broker that failed reopens before it
    /// reads again" is one assignment rather than a pair of flags that three
    /// call sites each have to remember to clear.
    ///
    /// The broker's name is attached here rather than carried in the error:
    /// the driver knows which broker it is driving, and an error that named
    /// itself would name whichever broker it was copied from.
    async fn pause(&mut self, error: BrokerError) -> ControlFlow<()> {
        self.broker.disconnect();
        self.contact = Contact::Closed;
        self.hub
            .publish_state(omega_proto::omega::StatePatch {
                topics: self
                    .broker
                    .topics()
                    .iter()
                    .map(|topic| omega_proto::omega::StateTopic {
                        topic: topic.as_str().to_owned(),
                        revision: 0,
                        value: None,
                    })
                    .collect(),
            })
            .unwrap_or_else(|error| tracing::error!(%error, "broker retraction refused"));

        let delay = self.backoff.delay();
        tracing::error!(
            broker = self.broker.name(),
            %error,
            ?delay,
            attempt = self.backoff.attempts(),
            "broker failed"
        );

        tokio::select! {
            biased;
            () = self.shutdown.wait() => ControlFlow::Break(()),
            _ = tokio::time::sleep(delay) => ControlFlow::Continue(()),
        }
    }
}

impl fmt::Debug for Driver {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Driver")
            .field("broker", &self.broker.name())
            .field("contact", &self.contact)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use omega_proto::SystemTopic;
    use omega_proto::omega::{Lock, action};
    use tokio::sync::{mpsc, oneshot};
    use tokio::time::timeout;

    use super::{Broker, Contact, Inbox, Request};

    /// Long enough that a wait which resolves is a defect, not a slow test.
    /// Under `start_paused` no wall clock passes: an idle runtime advances
    /// straight to the deadline.
    const NEVER: Duration = Duration::from_secs(3600);

    /// A broker that takes every default: it holds nothing, reports nothing,
    /// and never wakes.
    struct Silent;

    #[async_trait::async_trait]
    impl Broker for Silent {
        fn name(&self) -> &'static str {
            "silent"
        }

        fn topics(&self) -> &'static [SystemTopic] {
            &[]
        }
    }

    fn request() -> Request {
        let (answer, _) = oneshot::channel();
        Request {
            action: action::Kind::Lock(Lock {}),
            answer,
            deadline: tokio::time::Instant::now() + super::super::Brokerage::ACTION_TIMEOUT,
            _bytes: std::sync::Arc::new(tokio::sync::Semaphore::new(1))
                .try_acquire_owned()
                .unwrap(),
        }
    }

    struct Counting(std::sync::Arc<std::sync::atomic::AtomicUsize>);

    #[async_trait::async_trait]
    impl Broker for Counting {
        fn name(&self) -> &'static str {
            "counting"
        }
        fn topics(&self) -> &'static [SystemTopic] {
            &[]
        }
        async fn act(
            &mut self,
            _: &action::Kind,
        ) -> Result<Option<omega_proto::omega::StatePatch>, omega_brokers::BrokerError> {
            self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(None)
        }
    }

    #[tokio::test]
    async fn abandoned_requests_do_not_start_external_actions() {
        let count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let (_sender, receiver) = mpsc::channel(1);
        let mut driver = super::Driver::new(
            Box::new(Counting(count.clone())),
            receiver,
            crate::hub::Hub::new(),
            crate::shutdown::Shutdown::new(),
        );
        assert!(driver.serve(request()).await.is_continue());
        assert_eq!(count.load(std::sync::atomic::Ordering::SeqCst), 0);
        let (answer, receive) = oneshot::channel();
        assert!(
            driver
                .serve(Request {
                    action: action::Kind::Lock(Lock {}),
                    answer,
                    deadline: tokio::time::Instant::now() + super::super::Brokerage::ACTION_TIMEOUT,
                    _bytes: std::sync::Arc::new(tokio::sync::Semaphore::new(1))
                        .try_acquire_owned()
                        .unwrap(),
                })
                .await
                .is_continue()
        );
        receive.await.unwrap().unwrap();
        assert_eq!(count.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    struct Disconnected;

    #[async_trait::async_trait]
    impl Broker for Disconnected {
        fn name(&self) -> &'static str {
            "disconnected"
        }
        fn topics(&self) -> &'static [SystemTopic] {
            &[SystemTopic::Battery]
        }
    }

    #[tokio::test(start_paused = true)]
    async fn failure_retracts_the_brokers_readings() {
        let hub = crate::hub::Hub::new();
        hub.publish_state(omega_proto::omega::StatePatch {
            topics: vec![omega_proto::omega::StateTopic {
                topic: "battery".into(),
                revision: 0,
                value: Some(omega_proto::omega::state_topic::Value::Battery(
                    Default::default(),
                )),
            }],
        })
        .unwrap();
        let (_sender, receiver) = mpsc::channel(1);
        let mut driver = super::Driver::new(
            Box::new(Disconnected),
            receiver,
            hub.clone(),
            crate::shutdown::Shutdown::new(),
        );
        assert!(
            driver
                .pause(omega_brokers::BrokerError::unreadable("lost connection"))
                .await
                .is_continue()
        );
        assert!(hub.snapshot().topics[0].value.is_none());
    }

    #[derive(Clone)]
    struct Recovery {
        opens: std::sync::Arc<std::sync::atomic::AtomicUsize>,
        live: std::sync::Arc<std::sync::atomic::AtomicBool>,
        block: &'static str,
    }

    impl Recovery {
        fn new(block: &'static str) -> Self {
            Self {
                opens: Default::default(),
                live: Default::default(),
                block,
            }
        }
        fn driver(&self) -> super::Driver {
            let (_sender, receiver) = mpsc::channel(1);
            super::Driver::new(
                Box::new(self.clone()),
                receiver,
                crate::hub::Hub::new(),
                crate::shutdown::Shutdown::new(),
            )
        }
        fn is_live(&self) -> bool {
            self.live.load(std::sync::atomic::Ordering::SeqCst)
        }
    }

    #[async_trait::async_trait]
    impl Broker for Recovery {
        fn name(&self) -> &'static str {
            "recovery"
        }
        fn topics(&self) -> &'static [SystemTopic] {
            &[SystemTopic::Battery]
        }
        fn disconnect(&mut self) {
            self.live.store(false, std::sync::atomic::Ordering::SeqCst);
        }
        async fn connect(&mut self) -> Result<(), omega_brokers::BrokerError> {
            assert!(!self.is_live(), "reconnect retained old connection");
            self.live.store(true, std::sync::atomic::Ordering::SeqCst);
            let attempt = self.opens.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            if self.block == "connect" && attempt == 0 {
                std::future::pending().await
            } else {
                Ok(())
            }
        }
        async fn read(
            &mut self,
        ) -> Result<omega_proto::omega::StatePatch, omega_brokers::BrokerError> {
            if self.block == "read" && self.opens.load(std::sync::atomic::Ordering::SeqCst) == 1 {
                std::future::pending().await
            } else {
                Ok(Default::default())
            }
        }
        async fn act(
            &mut self,
            _: &action::Kind,
        ) -> Result<Option<omega_proto::omega::StatePatch>, omega_brokers::BrokerError> {
            std::future::pending().await
        }
    }

    #[tokio::test(start_paused = true)]
    async fn connection_deadline_releases_partial_connection_and_retries() {
        let broker = Recovery::new("connect");
        let mut driver = broker.driver();
        assert!(driver.open().await.is_continue());
        assert!(!broker.is_live());
        assert_eq!(driver.contact, Contact::Closed);
        assert!(driver.open().await.is_continue());
        assert!(broker.is_live());
        assert_eq!(driver.contact, Contact::Opened);
    }

    #[tokio::test(start_paused = true)]
    async fn reading_deadline_reconnects_before_reading_again() {
        let broker = Recovery::new("read");
        let mut driver = broker.driver();
        assert!(driver.open().await.is_continue());
        assert!(driver.read().await.is_continue());
        assert!(!broker.is_live());
        assert_eq!(driver.contact, Contact::Closed);
        assert!(driver.open().await.is_continue());
        assert!(driver.read().await.is_continue());
        assert_eq!(driver.contact, Contact::Primed);
    }

    #[tokio::test(start_paused = true)]
    async fn action_deadline_answers_and_discards_the_connection() {
        let broker = Recovery::new("action");
        let mut driver = broker.driver();
        assert!(driver.open().await.is_continue());
        let (answer, receive) = oneshot::channel();
        let mut request = request();
        request.answer = answer;
        let started = tokio::time::Instant::now();
        assert!(driver.serve(request).await.is_continue());
        assert!(matches!(
            receive.await.unwrap(),
            Err(omega_brokers::BrokerError::Timeout)
        ));
        assert!(tokio::time::Instant::now() - started >= super::super::Brokerage::ACTION_TIMEOUT);
        assert!(!broker.is_live());
        assert_eq!(driver.contact, Contact::Closed);
    }

    #[tokio::test(start_paused = true)]
    async fn an_expired_queued_action_is_answered_without_touching_the_broker() {
        let broker = Recovery::new("action");
        let mut driver = broker.driver();
        assert!(driver.open().await.is_continue());
        let (answer, receive) = oneshot::channel();
        let mut request = request();
        request.answer = answer;
        tokio::time::advance(super::super::Brokerage::ACTION_TIMEOUT).await;
        assert!(driver.serve(request).await.is_continue());
        assert!(matches!(
            receive.await.unwrap(),
            Err(omega_brokers::BrokerError::Timeout)
        ));
        assert!(broker.is_live());
    }

    struct Wedged;

    #[async_trait::async_trait]
    impl Broker for Wedged {
        fn name(&self) -> &'static str {
            "wedged"
        }
        fn topics(&self) -> &'static [SystemTopic] {
            &[]
        }
        async fn connect(&mut self) -> Result<(), omega_brokers::BrokerError> {
            std::future::pending().await
        }
    }

    #[tokio::test(start_paused = true)]
    async fn shutdown_interrupts_a_broker_stuck_connecting() {
        let shutdown = crate::shutdown::Shutdown::new();
        let (_sender, receiver) = mpsc::channel(1);
        let driver = super::Driver::new(
            Box::new(Wedged),
            receiver,
            crate::hub::Hub::new(),
            shutdown.clone(),
        );
        let task = tokio::spawn(driver.run());
        tokio::task::yield_now().await;
        shutdown.trigger();
        timeout(Duration::from_secs(1), task)
            .await
            .unwrap()
            .unwrap();
    }

    #[tokio::test(start_paused = true)]
    async fn a_closed_inbox_never_answers() {
        // The whole reason the type exists. A bare `Receiver` answers `None`
        // here on every poll, and the driver's select — which reaches the
        // broker's own wait only if this arm does not — reads in a tight
        // loop instead. Five of the twelve brokers claim no actions.
        let (requests, inbox) = mpsc::channel(1);
        drop(requests);
        let mut inbox = Inbox(inbox);

        assert!(
            timeout(NEVER, inbox.next()).await.is_err(),
            "a closed inbox answered, so the driver would never wait"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn an_inbox_that_is_only_empty_still_delivers() {
        // The other half: silence while a sender lives is a wait, not a
        // verdict, and the action that arrives after it is served.
        let (requests, inbox) = mpsc::channel(1);
        let mut inbox = Inbox(inbox);

        assert!(timeout(NEVER, inbox.next()).await.is_err());

        requests.send(request()).await.expect("the inbox is open");
        assert!(
            timeout(NEVER, inbox.next()).await.is_ok(),
            "an action was sent and the inbox did not deliver it"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn only_a_reporting_broker_waits_before_reading() {
        let mut broker = Silent;

        // Freshly opened: read at once rather than leave a widget blank
        // until the subsystem happens to change.
        assert!(Contact::Opened.due(&mut broker).await.is_ok());

        // Reporting: the broker says when, and this one never does.
        assert!(
            timeout(NEVER, Contact::Primed.due(&mut broker))
                .await
                .is_err()
        );

        // Closed is unreachable from `step`, and still says nothing rather
        // than something wrong.
        assert!(
            timeout(NEVER, Contact::Closed.due(&mut broker))
                .await
                .is_err()
        );
    }
}
