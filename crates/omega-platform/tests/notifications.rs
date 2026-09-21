//! Sending a notification.

mod common;

use omega_platform::notifications::Sent;
use omega_platform::{Broker, Notifications};
use omega_proto::omega::{Notify, action, state_topic};
use omega_proto::{ActionKind, SystemTopic};

fn notify(timeout_ms: u32) -> Notify {
    Notify {
        summary: "Battery low".into(),
        body: "15% remaining".into(),
        icon: "battery-quarter".into(),
        timeout_ms,
    }
}

#[test]
fn a_timeout_nobody_gave_is_the_desktops_own() {
    // Convert an omitted timeout to the service default, not the bus's never-expire value.
    assert_eq!(Sent::of(&notify(0)).timeout_ms, -1);
    assert_eq!(Sent::of(&notify(5_000)).timeout_ms, 5_000);
}

#[test]
fn the_words_are_carried_through_unchanged() {
    let sent = Sent::of(&notify(0));
    assert_eq!(sent.summary, "Battery low");
    assert_eq!(sent.body, "15% remaining");
    assert_eq!(sent.icon, "battery-quarter");
}

#[test]
fn it_claims_notifying_and_tracks_its_own_notifications() {
    let broker = Notifications::new();
    assert_eq!(broker.actions(), &[ActionKind::Notify]);
    assert_eq!(broker.topics(), &[SystemTopic::Notifications]);
}

#[tokio::test]
#[ignore = "puts a notification on screen; run with --ignored"]
async fn it_reaches_the_desktop_it_is_running_on() {
    let mut broker = Notifications::new();
    let sent = common::serve(&mut broker, &action::Kind::Notify(notify(2_000))).await;

    assert!(sent.is_ok(), "{sent:?}");
    let Some(patch) = sent.unwrap() else {
        panic!("raising a notification changes the tracked list");
    };
    assert_eq!(patch.topics[0].topic, "notifications");
    assert!(matches!(
        patch.topics[0].value,
        Some(state_topic::Value::Notifications(_))
    ));
}
