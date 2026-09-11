//! Schedules: the only thing in the runtime plane nobody asks for.
//!
//! Two halves, tested apart because they fail apart. The reconciler decides
//! *which* schedules should be running, and the mistake it can make is
//! restarting a timer that did not change — which shows up as a clock that
//! resets every time an unrelated unit connects. The runtime decides *when*,
//! and the mistake it can make is firing a schedule the document has stopped
//! declaring.

use std::time::Duration;

use tokio::sync::broadcast::error::TryRecvError;

use omega_daemon::broker::Brokerage;
use omega_daemon::hub::Hub;
use omega_daemon::reconcile::{ScheduleProvider, schedules::ScheduleChange};
use omega_daemon::schedule::Schedules;
use omega_daemon::shutdown::Shutdown;
use omega_daemon::units::UnitTable;
use omega_proto::Cadence;
use omega_proto::omega::{Event, EventKind, Schedule, StateDocument};

fn schedules() -> (Hub, Schedules) {
    let hub = Hub::new();
    let shutdown = Shutdown::new();
    let units = UnitTable::detached(hub.clone());
    let brokers = Brokerage::new(hub.clone(), shutdown.clone());

    (hub.clone(), Schedules::new(hub, units, brokers, shutdown))
}

fn document(schedules: impl IntoIterator<Item = Schedule>) -> StateDocument {
    StateDocument {
        schedules: schedules.into_iter().collect(),
        ..StateDocument::default()
    }
}

/// The next schedule firing, or a failure that says what arrived instead.
async fn fired(events: &mut omega_daemon::hub::history::Receiver<Event>) -> String {
    let event = tokio::time::timeout(Duration::from_secs(2), events.recv())
        .await
        .expect("timed out waiting for a schedule to fire")
        .expect("the hub is still broadcasting");

    assert_eq!(
        EventKind::try_from(event.kind),
        Ok(EventKind::EventScheduleFired)
    );
    event
        .schedule()
        .expect("a firing names the schedule that fired")
        .to_string()
}

#[tokio::test]
async fn a_schedule_fires_as_soon_as_it_starts() {
    // Not after one period. A schedule that waited out its first interval
    // would leave whatever it feeds empty until then, and "every ten minutes"
    // is not a request to start in ten minutes.
    let (hub, schedules) = schedules();
    let mut events = hub.subscribe_events();

    schedules
        .start(&Schedule::announcing("refresh", Cadence::minutes(10)))
        .await
        .unwrap();

    assert_eq!(fired(&mut events).await, "refresh");
}

#[tokio::test]
async fn a_schedule_with_no_action_still_announces_itself() {
    // The half a unit reacts to. A document that declares only a cadence is
    // saying how often, and leaving what to do about it to whoever listens.
    let (hub, schedules) = schedules();
    let mut events = hub.subscribe_events();

    let announcing = Schedule::announcing("tick", Cadence::seconds(30));
    assert!(announcing.action.is_none());
    schedules.start(&announcing).await.unwrap();

    assert_eq!(fired(&mut events).await, "tick");
}

#[tokio::test(start_paused = true)]
async fn a_schedule_that_was_stopped_does_not_fire_again() {
    let (hub, schedules) = schedules();
    let mut events = hub.subscribe_events();

    schedules
        .start(&Schedule::announcing("tick", Cadence::seconds(1)))
        .await
        .unwrap();
    assert_eq!(fired(&mut events).await, "tick");

    schedules.stop("tick").await;
    assert!(schedules.declared().is_empty());

    // Several periods, on a clock the test moves. A timer that outlived its
    // declaration would have fired at least twice by here.
    tokio::time::advance(Duration::from_secs(5)).await;
    tokio::task::yield_now().await;

    assert_eq!(
        events.try_recv().err(),
        Some(TryRecvError::Empty),
        "a stopped schedule kept firing"
    );
}

#[tokio::test]
async fn a_cadence_this_build_cannot_read_starts_nothing() {
    // The failure the grammar exists to prevent, at the boundary where a
    // document arrives from disk: a schedule that is accepted and then never
    // fires is worse than one that is refused.
    let (_hub, schedules) = schedules();

    let cron = Schedule {
        id: "nightly".into(),
        cadence: "0 9 * * *".into(),
        action: None,
    };

    let err = schedules.start(&cron).await.unwrap_err();
    assert!(err.to_string().contains("cron is not read"), "{err}");
    assert!(schedules.declared().is_empty());
}

#[tokio::test]
async fn a_schedule_that_changed_is_replaced_rather_than_doubled() {
    let (_hub, schedules) = schedules();

    schedules
        .start(&Schedule::announcing("tick", Cadence::minutes(1)))
        .await
        .unwrap();
    schedules
        .start(&Schedule::announcing("tick", Cadence::minutes(5)))
        .await
        .unwrap();

    let running = schedules.declared();
    assert_eq!(running.len(), 1, "one id is one timer");
    assert_eq!(running[0].cadence, "every 5m");
}

#[test]
fn a_document_that_declares_a_schedule_plans_to_start_it() {
    let (_hub, schedules) = schedules();
    let provider = ScheduleProvider::new(schedules);

    let plan = provider
        .plan(&document([Schedule::announcing(
            "refresh",
            Cadence::minutes(10),
        )]))
        .unwrap();

    assert_eq!(plan.len(), 1);
    assert_eq!(
        plan,
        vec![ScheduleChange::Set(Schedule::announcing(
            "refresh",
            Cadence::minutes(10)
        ))]
    );
}

#[tokio::test]
async fn a_schedule_that_did_not_change_is_left_alone() {
    // The whole reason the timers outlive a pass. Convergence runs whenever a
    // unit connects or a bar re-renders; a plan that restarted every schedule
    // each time would reset every clock on the machine, and a ten-minute
    // refresh would never reach ten minutes.
    let (_hub, schedules) = schedules();
    let document = document([Schedule::announcing("refresh", Cadence::minutes(10))]);

    let provider = ScheduleProvider::new(schedules);
    provider
        .apply(&provider.plan(&document).unwrap())
        .await
        .unwrap();

    assert!(
        provider.plan(&document).unwrap().is_empty(),
        "a converged document still planned work"
    );
}

#[tokio::test]
async fn a_schedule_whose_cadence_changed_is_planned_as_an_update() {
    let (_hub, schedules) = schedules();
    let provider = ScheduleProvider::new(schedules);

    let before = document([Schedule::announcing("refresh", Cadence::minutes(10))]);
    provider
        .apply(&provider.plan(&before).unwrap())
        .await
        .unwrap();

    let after = document([Schedule::announcing("refresh", Cadence::minutes(30))]);
    let plan = provider.plan(&after).unwrap();

    assert_eq!(plan.len(), 1);
    assert_eq!(
        plan,
        vec![ScheduleChange::Set(Schedule::announcing(
            "refresh",
            Cadence::minutes(30)
        ))]
    );
}

#[tokio::test]
async fn a_schedule_the_document_stops_declaring_is_planned_away() {
    let (_hub, schedules) = schedules();
    let provider = ScheduleProvider::new(schedules);

    let before = document([Schedule::announcing("refresh", Cadence::minutes(10))]);
    provider
        .apply(&provider.plan(&before).unwrap())
        .await
        .unwrap();

    let plan = provider.plan(&StateDocument::default()).unwrap();
    assert_eq!(plan.len(), 1);
    assert_eq!(plan, vec![ScheduleChange::Remove("refresh".into())]);

    provider.apply(&plan).await.unwrap();
    assert!(provider.plan(&StateDocument::default()).unwrap().is_empty());
}

#[tokio::test]
async fn a_schedule_performs_the_action_the_document_gave_it() {
    // `RunCommand` because it is the one action that needs neither a unit nor
    // a broker to prove it ran: the file is either there or it is not.
    let (_hub, schedules) = schedules();

    let dir = std::env::temp_dir().join(format!("omega-schedule-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let touched = dir.join("fired");
    let _ = std::fs::remove_file(&touched);

    schedules
        .start(&Schedule::new(
            "touch",
            Cadence::minutes(10),
            omega_document::Actions::run(format!("touch {}", touched.display())),
        ))
        .await
        .unwrap();

    // The action is spawned and not waited for, so this waits for the
    // effect rather than for the call.
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while !touched.exists() && std::time::Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    assert!(touched.exists(), "the schedule's action did not run");
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn provider_reports_failure_instead_of_claiming_convergence() {
    let (_, schedules) = schedules();
    schedules.shutdown().await;
    let provider = ScheduleProvider::new(schedules);
    let document = document([Schedule::announcing("tick", Cadence::seconds(1))]);
    assert!(
        provider
            .apply(&provider.plan(&document).unwrap())
            .await
            .is_err()
    );
    assert_eq!(provider.plan(&document).unwrap().len(), 1);
}

#[tokio::test(start_paused = true)]
async fn malformed_action_replacement_preserves_the_running_schedule() {
    let (hub, schedules) = schedules();
    let mut events = hub.subscribe_events();
    let original = Schedule::announcing("tick", omega_proto::Cadence::seconds(2));
    schedules.start(&original).await.unwrap();
    assert_eq!(fired(&mut events).await, "tick");
    let mut replacement = original.clone();
    replacement.action = Some(omega_proto::omega::Action {
        kind: Some(omega_proto::omega::action::Kind::SetBacklight(
            omega_proto::omega::SetBacklight {
                change: Some(omega_proto::omega::set_backlight::Change::AbsolutePercent(
                    101,
                )),
            },
        )),
    });
    assert!(matches!(
        schedules.start(&replacement).await,
        Err(omega_daemon::schedule::ScheduleError::Action(_))
    ));
    assert_eq!(schedules.declared(), vec![original]);
    tokio::time::advance(Duration::from_secs(2)).await;
    assert_eq!(fired(&mut events).await, "tick");
    schedules.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn invalid_schedule_plan_leaves_running_timers_untouched() {
    let (hub, schedules) = schedules();
    let provider = ScheduleProvider::new(schedules.clone());
    let original = Schedule::announcing("tick", Cadence::seconds(2));
    let mut events = hub.subscribe_events();
    schedules.start(&original).await.unwrap();
    assert_eq!(fired(&mut events).await, "tick");
    let mut invalid = original.clone();
    invalid.cadence = "0 9 * * *".into();
    assert!(provider.plan(&document([invalid])).is_err());
    assert!(
        provider
            .plan(&document([original.clone(), original.clone()]))
            .is_err()
    );
    assert_eq!(schedules.declared(), vec![original]);
    tokio::time::advance(Duration::from_secs(2)).await;
    assert_eq!(fired(&mut events).await, "tick");
    schedules.shutdown().await;
}

#[tokio::test]
async fn schedule_plan_carries_the_declaration_to_apply() {
    let (_, schedules) = schedules();
    let provider = ScheduleProvider::new(schedules.clone());
    let original = Schedule::announcing("tick", Cadence::seconds(2));
    let mut document = document([original.clone()]);
    let plan = provider.plan(&document).unwrap();
    document.schedules.clear();
    provider.apply(&plan).await.unwrap();
    assert_eq!(schedules.declared(), vec![original]);
    schedules.shutdown().await;
}
