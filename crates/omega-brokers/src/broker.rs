//! What a broker is.

use std::time::Duration;

use async_trait::async_trait;
use tokio::time::{Interval, MissedTickBehavior};

use omega_proto::omega::{StatePatch, action};
use omega_proto::{ActionKind, SystemTopic};

/// A subsystem, in both directions.
///
/// One broker owns one connection to one subsystem and is the only thing
/// that holds it. It projects that connection as topics and serves the
/// actions that write to it, because reading the volume and setting it are
/// one PipeWire connection — split across two components they become two
/// connections, two discovery paths, and no shared answer to whether
/// PipeWire is running at all.
///
/// [`topics`] and [`actions`] are static and declared whether or not the
/// broker is connected: what a subsystem covers is not a runtime discovery,
/// and the daemon has to know what nothing covers.
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

    /// The next change to report.
    ///
    /// Called in a loop. A polled broker awaits its interval and reads; a
    /// signal-driven one awaits its stream. Both are the same shape, so the
    /// daemon has one driver and cadence is the broker's business.
    ///
    /// **Must be cancel-safe.** The driver selects on this alongside
    /// shutdown and incoming actions, so the future is dropped and remade
    /// around anything else that happens. A `next` that buffers a partial
    /// read across awaits loses it.
    ///
    /// An `Err` is not fatal: the driver logs it, waits, and calls again.
    async fn next(&mut self) -> Result<StatePatch, BrokerError>;

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
    tick: Option<Interval>,
}

impl Cadence {
    pub(crate) fn every(every: Duration) -> Self {
        Self { every, tick: None }
    }

    /// Wait for the next turn. The first is immediate, so a broker reports
    /// once before its interval has elapsed. Cancel-safe.
    pub(crate) async fn wait(&mut self) {
        self.tick
            .get_or_insert_with(|| {
                let mut tick = tokio::time::interval(self.every);
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
    /// The subsystem is reachable but said something this broker cannot read.
    #[error("unreadable {subsystem}: {detail}")]
    Unreadable {
        subsystem: &'static str,
        detail: String,
    },
    /// Routed here by mistake: the kind is not one this broker declared.
    #[error("{} is not served by this broker", .0.name())]
    Unserved(ActionKind),
}
