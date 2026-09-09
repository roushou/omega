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
        while self.step().await.is_continue() {}
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
            Turn::Serve(request) => {
                self.serve(request).await;
                ControlFlow::Continue(())
            }
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
            // Shutdown first, so stopping does not depend on which of several
            // ready branches the runtime picks.
            biased;
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
        match self.broker.connect().await {
            Ok(()) => {
                self.contact = Contact::Opened;
                ControlFlow::Continue(())
            }
            Err(error) => self.pause(error).await,
        }
    }

    /// Take a reading and publish what it changed.
    async fn read(&mut self) -> ControlFlow<()> {
        match self.broker.read().await {
            Ok(patch) => {
                self.backoff.reset();
                // Only once a reading has worked: a read that failed or was
                // cancelled leaves the broker asking again rather than
                // waiting on a change it has already missed the state of.
                self.contact = Contact::Primed;
                self.hub.publish_state(patch);
                ControlFlow::Continue(())
            }
            Err(error) => self.pause(error).await,
        }
    }

    /// One action, and the answer to whoever asked.
    async fn serve(&mut self, request: Request) {
        let outcome = self.broker.act(&request.action).await;

        if let Ok(Some(patch)) = &outcome {
            // What the action changed, without waiting for the next wake to
            // notice it.
            self.hub.publish_state(patch.clone());
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
        let _ = request.answer.send(outcome.map(|_| ()));
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
        self.contact = Contact::Closed;

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
        }
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
