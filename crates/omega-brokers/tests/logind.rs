//! The session broker: what it claims, and what it refuses.
//!
//! Everything logind actually does ends the session, so there is no live test
//! here — one that passed would have suspended the machine running it. What
//! is testable is the part that decides: which actions this broker says it
//! serves, and that it refuses anything routed to it by mistake.

use omega_brokers::{Broker, Logind};
use omega_proto::ActionKind;
use omega_proto::omega::{RunCommand, action};

#[test]
fn it_claims_the_five_ways_a_session_ends() {
    let claimed = Logind::new().actions();

    for kind in [
        ActionKind::Lock,
        ActionKind::Sleep,
        ActionKind::Hibernate,
        ActionKind::Reboot,
        ActionKind::Shutdown,
    ] {
        assert!(claimed.contains(&kind), "{} is logind's", kind.name());
    }
    assert_eq!(claimed.len(), 5, "and nothing else");
}

#[test]
fn it_reports_whether_anybody_is_using_the_machine() {
    assert_eq!(Logind::new().topics(), &[omega_proto::SystemTopic::Idle]);
}

#[tokio::test]
async fn an_action_it_never_claimed_is_refused() {
    // The daemon routes by claim, so this is unreachable in practice. It is
    // still a refusal rather than a silent success: a broker that answered
    // `Ok` to something it did not do would report a machine that rebooted.
    let refused = Logind::new()
        .act(&action::Kind::RunCommand(RunCommand {
            command: "true".into(),
        }))
        .await;

    assert!(matches!(
        refused,
        Err(omega_brokers::BrokerError::Unserved(_))
    ));
}

// ---- against the machine this is running on ----

use omega_proto::omega::state_topic;
use std::time::Duration;

#[tokio::test]
#[ignore = "needs a system bus with logind; run with --ignored"]
async fn it_reads_the_session_it_is_running_in() {
    let mut logind = Logind::new();

    let patch = logind.next().await.expect("logind answered");
    assert_eq!(patch.topics[0].topic, "idle");

    let Some(state_topic::Value::Idle(idle)) = patch.topics[0].value.as_ref() else {
        panic!("expected an idle reading");
    };
    // A session running a test is being used, so it is not idle — and a
    // session that is not idle has not been idle since any particular moment.
    assert!(!idle.idle);
    assert_eq!(idle.idle_since, 0);

    let again = tokio::time::timeout(Duration::from_millis(500), logind.next()).await;
    assert!(again.is_err(), "a second reading should wait");
}
