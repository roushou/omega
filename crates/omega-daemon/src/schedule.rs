//! The schedules the daemon is running, and the clock they run on.
//!
//! A schedule is the one thing in the runtime plane that nobody asks for. A
//! press comes from a shell, a command comes from a caller, a state change
//! comes from a subsystem — a schedule comes from the document alone, which
//! is what makes it the answer to work that has to happen while nobody is
//! looking.
//!
//! Two things reach a unit when one fires, and they are not two ways of doing
//! the same thing:
//!
//! - the schedule's **action**, which is what the document said to do, and
//!   goes through the same [`Actions`] the shell and every unit go through;
//! - **`EVENT_SCHEDULE_FIRED`**, which is the announcement that it happened,
//!   heard by every unit that declared the event and told apart by id.
//!
//! A document that names an action gets the first. One that names none gets
//! only the second, which is how a unit reacts to a cadence its author does
//! not own.
//!
//! # Trust
//!
//! Nothing here is capability-checked, and that is the point rather than an
//! omission. Capabilities bound what a *unit* may ask the daemon for; a
//! schedule is not a unit asking, it is the machine's own document saying
//! what the machine does — the same standing as the bars it declares and the
//! units it enables. `Actions::authorize` is for the sessions on the far side
//! of the socket, and there is no session here.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tokio::task::JoinHandle;
use tokio::time::MissedTickBehavior;

use omega_proto::CadenceError;
use omega_proto::omega::Schedule;

use crate::action::Actions;
use crate::broker::Brokerage;
use crate::hub::Hub;
use crate::shutdown::Shutdown;
use crate::units::UnitTable;

/// The live schedules. Cloneable, and outlives any one convergence pass —
/// which is the whole reason it is a handle and not a field on the provider:
/// a reconciler is rebuilt on every pass, and timers that were rebuilt with
/// it would fire once and never again.
#[derive(Debug, Clone)]
pub struct Schedules {
    inner: Arc<Inner>,
}

#[derive(Debug)]
struct Inner {
    hub: Hub,
    units: UnitTable,
    brokers: Brokerage,
    shutdown: Shutdown,
    running: Mutex<HashMap<String, Running>>,
}

/// One schedule that is firing, and the declaration it is firing from.
///
/// The declaration is kept so a pass can tell a schedule that changed from
/// one that did not: the reconciler compares what the document says against
/// this, not against a timer it cannot read.
#[derive(Debug)]
struct Running {
    declared: Schedule,
    task: JoinHandle<()>,
}

impl Drop for Running {
    /// Dropping the entry stops the timer. A schedule is deleted by being
    /// taken out of the map — in one place, so there is no way to remove one
    /// and leave it ticking.
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl Schedules {
    pub fn new(hub: Hub, units: UnitTable, brokers: Brokerage, shutdown: Shutdown) -> Self {
        Self {
            inner: Arc::new(Inner {
                hub,
                units,
                brokers,
                shutdown,
                running: Mutex::new(HashMap::new()),
            }),
        }
    }

    fn running(&self) -> std::sync::MutexGuard<'_, HashMap<String, Running>> {
        self.inner.running.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// What is firing right now, as it was declared. Sorted, because a plan
    /// is read by a person.
    pub fn declared(&self) -> Vec<Schedule> {
        let mut schedules: Vec<Schedule> = self
            .running()
            .values()
            .map(|running| running.declared.clone())
            .collect();
        schedules.sort_by(|a, b| a.id.cmp(&b.id));
        schedules
    }

    /// Start firing a schedule, replacing any that already had its id.
    ///
    /// The first tick is immediate. A schedule that waited out its first
    /// period would leave whatever it feeds empty until then — ten minutes of
    /// a blank weather widget on every login — and "run this every ten
    /// minutes" is not usually a request to start in ten minutes.
    pub fn start(&self, schedule: &Schedule) -> Result<(), CadenceError> {
        let cadence = schedule.parsed()?;

        let firing = Firing {
            hub: self.inner.hub.clone(),
            units: self.inner.units.clone(),
            brokers: self.inner.brokers.clone(),
            shutdown: self.inner.shutdown.clone(),
            schedule: schedule.clone(),
        };

        let task = tokio::spawn(firing.run(cadence.period()));

        // Insert replaces, and the entry it displaces aborts its own timer as
        // it drops — so a schedule whose cadence changed does not end up
        // firing on both.
        self.running().insert(
            schedule.id.clone(),
            Running {
                declared: schedule.clone(),
                task,
            },
        );
        Ok(())
    }

    /// Stop firing a schedule. Silent on one that is not running: a document
    /// that no longer declares a schedule the daemon never started is already
    /// converged.
    pub fn stop(&self, id: &str) {
        self.running().remove(id);
    }
}

/// One schedule's timer.
struct Firing {
    hub: Hub,
    units: UnitTable,
    brokers: Brokerage,
    shutdown: Shutdown,
    schedule: Schedule,
}

impl Firing {
    async fn run(self, period: std::time::Duration) {
        let mut ticks = tokio::time::interval(period);

        // A slow action must not be repaid with a burst of catch-up ticks:
        // an action that takes longer than its period is a schedule asking
        // for more than the machine can give, and the answer is to fire less
        // often rather than to queue.
        ticks.set_missed_tick_behavior(MissedTickBehavior::Delay);

        loop {
            tokio::select! {
                _ = ticks.tick() => self.fire().await,
                _ = self.shutdown.wait() => return,
            }
        }
    }

    async fn fire(&self) {
        tracing::debug!(schedule = %self.schedule.id, "schedule fired");

        // Announced first, and whatever the action does. An event is what
        // happened, and the firing happened even if what it asked for could
        // not be done.
        self.hub.publish_schedule_fired(&self.schedule.id);

        let Some(action) = self
            .schedule
            .action
            .as_ref()
            .and_then(|action| action.kind.as_ref())
        else {
            return;
        };

        if let Err(refusal) = Actions::new(self.units.clone(), self.brokers.clone())
            .perform(action)
            .await
        {
            // Logged and dropped: there is nobody to answer. A schedule that
            // stopped itself on a failure would be a weather refresh that
            // gives up for good the first time the network is down.
            tracing::warn!(
                schedule = %self.schedule.id,
                action = omega_proto::ActionKind::of(action).name(),
                error = %refusal,
                "the schedule's action was refused",
            );
        }
    }
}
