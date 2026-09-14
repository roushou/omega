//! Recurring document actions and schedule events.
//! Each tick dispatches the optional action and broadcasts `EVENT_SCHEDULE_FIRED`.
//! Schedule declarations are trusted configuration; peer capability checks apply
//! at session dispatch, not to document-triggered execution.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tokio::task::{AbortHandle, JoinHandle};
use tokio::time::MissedTickBehavior;

use omega_proto::CadenceError;
use omega_proto::omega::Schedule;

use crate::action::Actions;
use crate::broker::Brokerage;
use crate::hub::Hub;
use crate::shutdown::Shutdown;
use crate::units::UnitTable;

/// Live timer registry shared across convergence passes.
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
    timers: Mutex<Timers>,
    changes: tokio::sync::Mutex<()>,
}

#[derive(Debug, Default)]
struct Timers {
    closed: bool,
    running: HashMap<String, Arc<Running>>,
}

#[derive(Debug, thiserror::Error)]
pub enum ScheduleError {
    #[error(transparent)]
    Action(#[from] omega_proto::action::ActionError),
    #[error(transparent)]
    Cadence(#[from] CadenceError),
    #[error("schedule period exceeds the monotonic clock range")]
    PeriodOutOfRange,
    #[error("schedules are shutting down")]
    Stopped,
}

#[derive(Debug)]
struct Running {
    declared: Schedule,
    task: tokio::sync::Mutex<JoinHandle<()>>,
    abort: AbortHandle,
}

impl Running {
    async fn stop(&self) {
        self.abort.abort();
        if let Err(error) = (&mut *self.task.lock().await).await
            && !error.is_cancelled()
        {
            tracing::error!(schedule = %self.declared.id, %error, "schedule task failed");
        }
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        self.abort.abort();
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
                timers: Mutex::new(Timers::default()),
                changes: tokio::sync::Mutex::new(()),
            }),
        }
    }

    fn timers(&self) -> std::sync::MutexGuard<'_, Timers> {
        self.inner.timers.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Return current declarations sorted by schedule ID.
    pub fn declared(&self) -> Vec<Schedule> {
        let mut schedules: Vec<Schedule> = self
            .timers()
            .running
            .values()
            .map(|running| running.declared.clone())
            .collect();
        schedules.sort_by(|a, b| a.id.cmp(&b.id));
        schedules
    }

    /// Start a schedule with an immediate first tick, replacing any timer with the same ID.
    pub async fn start(&self, schedule: &Schedule) -> Result<(), ScheduleError> {
        let cadence = schedule.parsed()?;
        tokio::time::Instant::now()
            .checked_add(cadence.period())
            .ok_or(ScheduleError::PeriodOutOfRange)?;
        if let Some(action) = &schedule.action {
            action.validate()?;
        }
        let _change = self.inner.changes.lock().await;
        if self.timers().closed || self.inner.shutdown.is_triggered() {
            return Err(ScheduleError::Stopped);
        }
        self.remove(&schedule.id).await;
        if self.inner.shutdown.is_triggered() {
            return Err(ScheduleError::Stopped);
        }

        let firing = Firing {
            hub: self.inner.hub.clone(),
            units: self.inner.units.clone(),
            brokers: self.inner.brokers.clone(),
            shutdown: self.inner.shutdown.clone(),
            schedule: schedule.clone(),
        };

        let shutdown = self.inner.shutdown.clone();
        let task_name = format!("schedule {}", schedule.id);
        let task = tokio::spawn(async move {
            shutdown
                .supervise(task_name, firing.run(cadence.period()))
                .await;
        });

        self.timers().running.insert(
            schedule.id.clone(),
            Arc::new(Running {
                declared: schedule.clone(),
                abort: task.abort_handle(),
                task: tokio::sync::Mutex::new(task),
            }),
        );
        Ok(())
    }

    /// Stop a running schedule. A missing schedule is already converged.
    pub async fn stop(&self, id: &str) {
        let _change = self.inner.changes.lock().await;
        self.remove(id).await;
    }

    // The mutation gate stays held, and the registry retains ownership across
    // the join: cancelling the caller must not detach the old task.
    async fn remove(&self, id: &str) {
        let running = self.timers().running.get(id).cloned();
        if let Some(running) = running {
            running.stop().await;
            self.timers().running.remove(id);
        }
    }

    /// Close admission and join every timer, including any action it awaits.
    pub async fn shutdown(&self) {
        let _change = self.inner.changes.lock().await;
        self.timers().closed = true;
        loop {
            let id = self.timers().running.keys().next().cloned();
            let Some(id) = id else { return };
            self.remove(&id).await;
        }
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

        // Skip missed ticks rather than queueing catch-up actions.
        ticks.set_missed_tick_behavior(MissedTickBehavior::Delay);

        loop {
            tokio::select! {
                biased;
                _ = self.shutdown.wait() => return,
                _ = ticks.tick() => {}
            }
            tokio::select! {
                biased;
                _ = self.shutdown.wait() => return,
                _ = self.fire() => {}
            }
        }
    }

    async fn fire(&self) {
        tracing::debug!(schedule = %self.schedule.id, "schedule fired");

        // Publish the firing event even if the subsequent action fails.
        if let Err(error) = self.hub.publish_schedule_fired(&self.schedule.id) {
            tracing::error!(%error, schedule = %self.schedule.id, "schedule event refused");
        }

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
            // Log action failure and retain the schedule for subsequent ticks.
            tracing::warn!(
                schedule = %self.schedule.id,
                action = omega_proto::ActionKind::of(action).name(),
                error = %refusal,
                "the schedule's action was refused",
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use omega_proto::Cadence;

    struct Fixture {
        schedules: Schedules,
        shutdown: Shutdown,
    }

    impl Fixture {
        fn new() -> Self {
            let hub = Hub::new();
            let shutdown = Shutdown::new();
            let units = UnitTable::detached(hub.clone());
            let schedules = Schedules::new(
                hub.clone(),
                units,
                Brokerage::new(hub, shutdown.clone()),
                shutdown.clone(),
            );
            Self {
                schedules,
                shutdown,
            }
        }
    }

    struct BlockedAction(Arc<tokio::sync::Notify>);

    #[async_trait::async_trait]
    impl omega_platform::Broker for BlockedAction {
        fn name(&self) -> &'static str {
            "blocked-action"
        }
        fn topics(&self) -> &'static [omega_proto::SystemTopic] {
            &[]
        }
        fn actions(&self) -> &'static [omega_proto::ActionKind] {
            &[omega_proto::ActionKind::Lock]
        }
        async fn act(
            &mut self,
            _: &omega_proto::omega::action::Kind,
        ) -> Result<Option<omega_proto::omega::StatePatch>, omega_platform::BrokerError> {
            self.0.notify_one();
            std::future::pending().await
        }
    }

    #[tokio::test(start_paused = true)]
    async fn shutdown_interrupts_a_timer_awaiting_an_action() {
        let hub = Hub::new();
        let broker_shutdown = Shutdown::new();
        let brokers = Brokerage::new(hub.clone(), broker_shutdown.clone());
        let entered = Arc::new(tokio::sync::Notify::new());
        brokers.add(Box::new(BlockedAction(entered.clone())));
        let shutdown = Shutdown::new();
        let firing = Firing {
            units: UnitTable::detached(hub.clone()),
            hub,
            brokers: brokers.clone(),
            shutdown: shutdown.clone(),
            schedule: Schedule::new(
                "lock",
                Cadence::seconds(1),
                omega_proto::omega::Action {
                    kind: Some(omega_proto::omega::action::Kind::Lock(
                        omega_proto::omega::Lock {},
                    )),
                },
            ),
        };
        let task = tokio::spawn(firing.run(std::time::Duration::from_secs(1)));
        tokio::time::timeout(std::time::Duration::from_secs(1), entered.notified())
            .await
            .unwrap();
        shutdown.trigger();
        tokio::time::timeout(std::time::Duration::from_secs(1), task)
            .await
            .unwrap()
            .unwrap();
        broker_shutdown.trigger();
        brokers.stop().await;
    }

    #[tokio::test]
    async fn cancelling_replacement_retains_the_old_task_until_joined() {
        let fixture = Fixture::new();
        let schedules = &fixture.schedules;
        let old = Schedule::announcing("tick", Cadence::seconds(1));
        schedules.start(&old).await.unwrap();
        let running = schedules.timers().running["tick"].clone();
        let task = running.task.lock().await;
        let replacement = Schedule::announcing("tick", Cadence::seconds(2));
        {
            let start = schedules.start(&replacement);
            tokio::pin!(start);
            tokio::select! {
                biased;
                result = &mut start => panic!("replacement completed before joining: {result:?}"),
                _ = tokio::task::yield_now() => {}
            }
        }
        assert_eq!(schedules.declared(), vec![old]);
        drop(task);
        schedules.start(&replacement).await.unwrap();
        assert!(running.abort.is_finished());
        assert_eq!(schedules.declared(), vec![replacement]);
        schedules.shutdown().await;
        assert!(schedules.declared().is_empty());
    }

    #[tokio::test]
    async fn shutdown_joins_timers_and_closes_admission_on_every_handle() {
        let fixture = Fixture::new();
        let schedule = Schedule::announcing("tick", Cadence::seconds(1));
        fixture.schedules.start(&schedule).await.unwrap();
        let running = fixture.schedules.timers().running["tick"].clone();
        let other = fixture.schedules.clone();
        fixture.schedules.shutdown().await;
        assert!(running.abort.is_finished());
        assert!(other.declared().is_empty());
        assert!(matches!(
            other.start(&schedule).await,
            Err(ScheduleError::Stopped)
        ));
        other.shutdown().await;
    }

    #[tokio::test(start_paused = true)]
    async fn an_unrepresentable_period_preserves_the_existing_timer() {
        let fixture = Fixture::new();
        let original = Schedule::announcing("tick", Cadence::seconds(1));
        fixture.schedules.start(&original).await.unwrap();
        let replacement = Schedule {
            id: "tick".into(),
            cadence: format!("every {}s", u64::MAX),
            action: None,
        };
        assert!(matches!(
            fixture.schedules.start(&replacement).await,
            Err(ScheduleError::PeriodOutOfRange)
        ));
        assert_eq!(fixture.schedules.declared(), vec![original]);
        tokio::time::advance(std::time::Duration::from_secs(2)).await;
        assert!(!fixture.shutdown.is_triggered());
        fixture.schedules.shutdown().await;
    }

    #[tokio::test]
    async fn a_triggered_shutdown_refuses_new_timers() {
        let fixture = Fixture::new();
        fixture.shutdown.trigger();
        assert!(matches!(
            fixture
                .schedules
                .start(&Schedule::announcing("tick", Cadence::seconds(1)))
                .await,
            Err(ScheduleError::Stopped)
        ));
    }
}
