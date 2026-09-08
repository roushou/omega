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
fn it_reports_nothing() {
    // A broker may exist only to be asked. Whether the session is idle or
    // locked is worth a topic and does not have one yet, and claiming one it
    // does not fill would make a unit wait forever on it.
    assert!(Logind::new().topics().is_empty());
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
