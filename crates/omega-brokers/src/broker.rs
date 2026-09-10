//! What a broker is.

use std::time::Duration;

use async_trait::async_trait;
use tokio::time::{Interval, MissedTickBehavior};

use omega_proto::omega::{StatePatch, action};
use omega_proto::{ActionKind, SystemTopic};

/// A subsystem, in both directions.
///
/// One broker owns the only connection to one subsystem: it projects that
/// connection as topics and serves the actions that write to it.
///
/// # Connect, wake, read
///
/// A broker says how to do those three; the driver owns when, and holds the
/// rules for all of them:
///
///  - open lazily, and again after the connection goes;
///  - take the first reading at once rather than waiting for a change;
///  - count a broker primed only *after* a reading succeeds, so a cancelled
///    wait asks again instead of waiting on a change it has already missed;
///  - drop the connection when whatever was being waited on closes.
///
/// The defaults are the taxonomy. Override nothing but [`act`] and this is a
/// broker that only serves — `logind`, `desktop`. Override [`wake`] and
/// [`read`] and it is polled — `procfs`, `backlight`. Override [`connect`]
/// too and it holds something.
///
/// [`act`]: Self::act
/// [`wake`]: Self::wake
/// [`read`]: Self::read
/// [`connect`]: Self::connect
///
/// [`topics`] and [`actions`] are static and declared whether or not the
/// broker is connected: the daemon has to know what nothing covers.
///
/// [`topics`]: Self::topics
/// [`actions`]: Self::actions
#[async_trait]
pub trait Broker: Send + 'static {
    /// For logs and `omega status`. Names the subsystem, not the topic —
    /// `"upower"`, not `"battery"`.
    fn name(&self) -> &'static str;

    /// The topics this broker projects. One connection often serves several:
    /// UPower reports the battery *and* every peripheral's.
    fn topics(&self) -> &'static [SystemTopic];

    /// The action kinds this broker serves. Empty for a broker that only
    /// reports.
    fn actions(&self) -> &'static [ActionKind] {
        &[]
    }

    /// Open whatever this broker holds.
    ///
    /// Called before the first reading, and again after [`wake`] or [`read`]
    /// reports the connection gone. A broker with nothing to hold — the
    /// clock, `/proc` — takes the default and does nothing.
    ///
    /// [`wake`]: Self::wake
    /// [`read`]: Self::read
    async fn connect(&mut self) -> Result<(), BrokerError> {
        Ok(())
    }

    /// Wait until there may be something new.
    ///
    /// A signal stream, an interval, or both. The default never returns,
    /// which is right for a broker that only serves actions: the driver
    /// selects on this and simply never wakes on that branch.
    ///
    /// **Must be cancel-safe.** The driver selects on it alongside shutdown
    /// and incoming actions, so the future is dropped and remade around
    /// anything else that happens. A `wake` that buffers a partial read
    /// across awaits loses it.
    ///
    /// An `Err` means the thing being waited on is gone: the driver reopens
    /// before the next reading.
    async fn wake(&mut self) -> Result<(), BrokerError> {
        std::future::pending().await
    }

    /// Take a reading.
    ///
    /// The driver calls this once after connecting — a broker's first
    /// reading is taken at once rather than waited for, or a bar is blank
    /// until something moves — and once after every [`wake`].
    ///
    /// [`wake`]: Self::wake
    async fn read(&mut self) -> Result<StatePatch, BrokerError> {
        Ok(StatePatch::default())
    }

    /// Serve one action, and report what it changed.
    ///
    /// A broker that just set the brightness knows the new value; making the
    /// caller wait for the next poll to see it is a slider that lags its own
    /// drag. Returning `None` means nothing observable changed.
    ///
    /// Only kinds this broker declared in [`actions`] reach it, so the
    /// default is unreachable rather than a silent no-op.
    ///
    /// [`actions`]: Self::actions
    async fn act(&mut self, action: &action::Kind) -> Result<Option<StatePatch>, BrokerError> {
        Err(BrokerError::Unserved(ActionKind::of(action)))
    }
}

/// A poll interval, built on first use.
///
/// A timer needs a runtime and a broker is constructed before there is one,
/// so the interval cannot be made in `new`. Shared because every polled
/// broker wants the same three lines, and one that set a different missed-tick
/// behaviour by accident would drift under load instead of skipping.
#[derive(Debug)]
pub(crate) struct Cadence {
    every: Duration,
    /// Whether the first turn comes at once or after a period.
    immediate: bool,
    tick: Option<Interval>,
}

impl Cadence {
    /// Every period, starting now.
    ///
    /// For a broker the clock drives: the first turn is immediate, so it
    /// reports once before its interval has elapsed rather than leaving a
    /// widget blank for it.
    pub(crate) fn every(every: Duration) -> Self {
        Self {
            every,
            immediate: true,
            tick: None,
        }
    }

    /// Every period, starting one period from now.
    ///
    /// For a cadence that is a floor under signals rather than the thing
    /// driving the broker. The reading has already been taken by the time
    /// this is first awaited, so a turn that came at once would make the
    /// broker report twice in a row and read as a hot loop.
    pub(crate) fn after(every: Duration) -> Self {
        Self {
            every,
            immediate: false,
            tick: None,
        }
    }

    /// Wait for the next turn. Cancel-safe.
    pub(crate) async fn wait(&mut self) {
        let every = self.every;
        let immediate = self.immediate;
        self.tick
            .get_or_insert_with(|| {
                let start = if immediate {
                    tokio::time::Instant::now()
                } else {
                    tokio::time::Instant::now() + every
                };
                let mut tick = tokio::time::interval_at(start, every);
                tick.set_missed_tick_behavior(MissedTickBehavior::Skip);
                tick
            })
            .tick()
            .await;
    }
}

#[derive(Debug, thiserror::Error)]
pub enum BrokerError {
    #[error("{0}")]
    Io(#[from] std::io::Error),
    /// The subsystem is reachable but said something this broker cannot read,
    /// or stopped saying anything.
    ///
    /// Which subsystem is not carried here. The driver knows which broker it
    /// is driving and attaches the name when it logs; a broker that named
    /// itself would name whichever broker it was copied from, and nothing
    /// would catch it because the field is only ever printed.
    #[error("{0}")]
    Unreadable(String),
    /// Routed here by mistake: the kind is not one this broker declared.
    #[error("{} is not served by this broker", .0.name())]
    Unserved(ActionKind),
}

impl BrokerError {
    /// Whatever a subsystem's own client said went wrong.
    pub fn unreadable(error: impl std::fmt::Display) -> Self {
        Self::Unreadable(error.to_string())
    }

    /// The connection is not there — closed under us, or never opened. The
    /// driver reopens before the next reading.
    pub fn gone() -> Self {
        Self::Unreadable("the connection is gone".into())
    }
}

/// Give a type a `Debug` that says only its name.
///
/// Every broker holds a connection whose parts do not implement `Debug` — a
/// zbus `Proxy`, a message stream, a child process — and every broker needs
/// `Debug` because the daemon holds it in a struct that derives it. Eight
/// copies of the same four lines, and none of them had anything to say.
macro_rules! opaque_debug {
    ($type:ident) => {
        impl std::fmt::Debug for $type {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(stringify!($type))
            }
        }
    };
}

pub(crate) use opaque_debug;
