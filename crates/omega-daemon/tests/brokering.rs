//! What the daemon does with a broker between its readings.
//!
//! The driver had no test of its own, and the hole was the shape of the bug
//! that lived in it: every broker registered by a test claimed an action, so
//! the path taken by a broker that only reports was never once driven. Five
//! of the twelve real brokers take it.
//!
//! Pacing is asserted on a clock the test moves — `start_paused`, per the
//! daemon's rule — so what these measure is the driver's behaviour and not
//! the machine's load.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use omega_brokers::{Broker, BrokerError};
use omega_daemon::broker::Brokerage;
use omega_daemon::hub::Hub;
use omega_daemon::shutdown::Shutdown;
use omega_proto::ActionKind;
use omega_proto::omega::{Lock, StatePatch, action};

/// The cadence the doubles below report on.
const TICK: Duration = Duration::from_secs(2);

/// Long enough that a wait which resolves is a defect. No wall clock passes:
/// an idle paused runtime advances straight to the deadline.
const NEVER: Duration = Duration::from_secs(3600);

/// A broker on a fixed cadence that counts what the driver asks of it.
///
/// What it claims is the whole variable: a broker that serves actions and one
/// that only reports differ in nothing else here, and the driver owes them
/// the same pacing.
#[derive(Clone)]
struct Metronome {
    reads: Arc<AtomicU32>,
    acted: Arc<AtomicU32>,
    claims: &'static [ActionKind],
    /// Fail every reading, to drive the reopen path.
    breaks: bool,
}

impl Metronome {
    fn reporting() -> Self {
        Self {
            reads: Arc::new(AtomicU32::new(0)),
            acted: Arc::new(AtomicU32::new(0)),
            claims: &[],
            breaks: false,
        }
    }

    fn serving() -> Self {
        Self {
            claims: &[ActionKind::Lock],
            ..Self::reporting()
        }
    }

    fn broken() -> Self {
        Self {
            breaks: true,
            ..Self::reporting()
        }
    }

    fn reads(&self) -> u32 {
        self.reads.load(Ordering::SeqCst)
    }

    fn acted(&self) -> u32 {
        self.acted.load(Ordering::SeqCst)
    }
}

#[async_trait::async_trait]
impl Broker for Metronome {
    fn name(&self) -> &'static str {
        "metronome"
    }

    fn topics(&self) -> &'static [omega_proto::SystemTopic] {
        &[]
    }

    fn actions(&self) -> &'static [ActionKind] {
        self.claims
    }

    async fn wake(&mut self) -> Result<(), BrokerError> {
        tokio::time::sleep(TICK).await;
        Ok(())
    }

    async fn read(&mut self) -> Result<StatePatch, BrokerError> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        if self.breaks {
            return Err(BrokerError::Unreadable("nothing to read".into()));
        }
        Ok(StatePatch::default())
    }

    async fn act(&mut self, _: &action::Kind) -> Result<Option<StatePatch>, BrokerError> {
        self.acted.fetch_add(1, Ordering::SeqCst);
        Ok(None)
    }
}

fn brokerage() -> (Brokerage, Shutdown) {
    let shutdown = Shutdown::new();
    (Brokerage::new(Hub::new(), shutdown.clone()), shutdown)
}

/// Let a driver start and settle: it connects and takes its first reading
/// before any time has to pass.
async fn settle() {
    for _ in 0..8 {
        tokio::task::yield_now().await;
    }
}

/// Let `ticks` of the cadence go by, one at a time so each is a wake the
/// driver can answer.
async fn elapse(ticks: u32) {
    for _ in 0..ticks {
        tokio::time::advance(TICK).await;
        settle().await;
    }
}

#[tokio::test(start_paused = true)]
async fn the_first_reading_is_taken_before_the_first_wake() {
    // A freshly connected broker reports at once. Waiting for a change
    // leaves a bar blank until the subsystem happens to move.
    let broker = Metronome::reporting();
    let (brokers, _shutdown) = brokerage();
    brokers.add(Box::new(broker.clone()));

    settle().await;

    assert_eq!(broker.reads(), 1, "no reading was taken on connecting");
}

#[tokio::test(start_paused = true)]
async fn a_broker_that_claims_no_actions_waits_between_readings() {
    // The regression. Such a broker is routed nothing, so its inbox closes
    // as soon as it is registered — and a closed inbox that answers is an
    // arm of the driver's select that wins every poll, which reads in a
    // tight loop and never reaches the wait below.
    let broker = Metronome::reporting();
    let (brokers, _shutdown) = brokerage();
    brokers.add(Box::new(broker.clone()));

    settle().await;
    elapse(3).await;

    assert_eq!(
        broker.reads(),
        4,
        "a broker claiming no actions read {} times in three ticks; \
         the driver is not waiting between readings",
        broker.reads()
    );
}

#[tokio::test(start_paused = true)]
async fn claiming_an_action_does_not_change_the_pacing() {
    // The control. Whether a broker serves actions is not supposed to be
    // visible in how often it is read — and it was: this one paced
    // correctly the whole time the one above was spinning.
    let broker = Metronome::serving();
    let (brokers, _shutdown) = brokerage();
    brokers.add(Box::new(broker.clone()));

    settle().await;
    elapse(3).await;

    assert_eq!(broker.reads(), 4);
}

#[tokio::test(start_paused = true)]
async fn an_action_is_served_without_waiting_for_the_cadence() {
    // A broker mid-wait is still reachable: the volume key does not queue
    // behind a thirty-second signal.
    let broker = Metronome::serving();
    let (brokers, _shutdown) = brokerage();
    brokers.add(Box::new(broker.clone()));

    settle().await;
    let before = broker.reads();

    let outcome = tokio::time::timeout(NEVER, brokers.act(&action::Kind::Lock(Lock {})))
        .await
        .expect("the broker was waiting out its cadence instead of answering");

    assert!(matches!(outcome, Some(Ok(()))));
    assert_eq!(broker.acted(), 1);
    assert_eq!(
        broker.reads(),
        before,
        "serving an action should not have taken a reading with it"
    );
}

#[tokio::test(start_paused = true)]
async fn a_failed_reading_is_retried_behind_a_backoff() {
    // A subsystem that went away comes back, so a failure reopens rather
    // than ending the driver — but not faster than the backoff allows, or a
    // broker whose connection is gone spins on reconnecting instead.
    let broker = Metronome::broken();
    let (brokers, _shutdown) = brokerage();
    brokers.add(Box::new(broker.clone()));

    settle().await;
    assert_eq!(broker.reads(), 1, "the first reading was never attempted");

    // Well inside the first backoff: nothing should have been retried yet.
    tokio::time::advance(Duration::from_millis(100)).await;
    settle().await;
    assert_eq!(
        broker.reads(),
        1,
        "a failed reading was retried immediately"
    );

    // Past it: the broker is asked again.
    elapse(4).await;
    assert!(
        broker.reads() > 1,
        "a failed reading was never retried at all"
    );
}

#[tokio::test(start_paused = true)]
async fn a_driver_stops_when_the_daemon_does() {
    // Shutdown is the one thing that ends a driver, and it is first in the
    // select so that stopping does not depend on what else is ready.
    let broker = Metronome::reporting();
    let (brokers, shutdown) = brokerage();
    brokers.add(Box::new(broker.clone()));

    settle().await;
    shutdown.trigger();

    tokio::time::timeout(NEVER, brokers.stop())
        .await
        .expect("a broker did not stop when the daemon did");
}
