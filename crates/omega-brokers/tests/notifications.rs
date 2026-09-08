//! Sending a notification.

mod common;

use omega_brokers::notifications::Sent;
use omega_brokers::{Broker, Notifications};
use omega_proto::ActionKind;
use omega_proto::omega::{Notify, action};

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
    // Zero is "not given" on the wire and "never expire" on the bus. A unit
    // that said nothing wants the desktop's default, not a notification that
    // stays on screen until somebody clicks it.
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
fn it_claims_only_notifying() {
    let broker = Notifications::new();
    assert_eq!(broker.actions(), &[ActionKind::Notify]);
    assert!(broker.topics().is_empty());
}

#[tokio::test]
#[ignore = "puts a notification on screen; run with --ignored"]
async fn it_reaches_the_desktop_it_is_running_on() {
    let mut broker = Notifications::new();
    let sent = common::serve(&mut broker, &action::Kind::Notify(notify(2_000))).await;

    assert!(sent.is_ok(), "{sent:?}");
    // Nothing observable changed that this broker reports: it has no topics.
    assert!(sent.unwrap().is_none());
}
