//! What the ontology declares against what a broker actually serves.
//!
//! The SDK publishes a handle per topic and per effect. Nothing tied those
//! handles to an implementation, so `Network` and `Audio` shipped as reads of
//! topics no producer filled — a unit holding one never draws, because the
//! runtime waits for a value that never arrives — and `Volume`, `Session` and
//! `Notify` shipped as writes the daemon answers `UNIMPLEMENTED`.
//!
//! This is the check that was missing. Every topic and every action is
//! either served by a broker or named below, so a hole is a line in this file
//! rather than a widget that hangs. A broker landing deletes lines from it.

use std::collections::HashSet;

use omega_brokers::Brokers;
use omega_proto::{ActionKind, SystemTopic};

/// Topics no broker projects yet.
const UNSERVED_TOPICS: &[SystemTopic] = &[];

/// Topics the daemon fills itself rather than through a broker.
///
/// Not a subsystem: the supervisor's report on the units it runs is its own
/// projection of its own table, and a broker for it would be the daemon
/// asking itself.
const DAEMON_TOPICS: &[SystemTopic] = &[SystemTopic::Units];

/// Actions nothing serves yet.
///
/// `RunCommand` and `InvokeUnit` are absent because the daemon performs them
/// itself: spawning a process is not brokering a subsystem, and routing
/// between units is the daemon's own job.
///
/// `SetSetting` and `ToggleSetting` are on this list rather than the daemon's
/// because nothing performs them. They are the daemon's to serve when
/// something does — they act on the state document's own settings, which no
/// subsystem owns.
const UNSERVED_ACTIONS: &[ActionKind] = &[
    ActionKind::LaunchApp,
    ActionKind::SetSetting,
    ActionKind::ToggleSetting,
    ActionKind::Screenshot,
];

/// Actions the daemon performs itself rather than through a broker.
const DAEMON_ACTIONS: &[ActionKind] = &[ActionKind::RunCommand, ActionKind::InvokeUnit];

fn served_topics() -> HashSet<SystemTopic> {
    Brokers::all()
        .iter()
        .flat_map(|broker| broker.topics().iter().copied())
        .collect()
}

fn served_actions() -> HashSet<ActionKind> {
    Brokers::all()
        .iter()
        .flat_map(|broker| broker.actions().iter().copied())
        .collect()
}

#[test]
fn every_topic_is_served_or_named_unserved() {
    let served = served_topics();
    let unserved: HashSet<SystemTopic> = UNSERVED_TOPICS.iter().copied().collect();
    let daemons: HashSet<SystemTopic> = DAEMON_TOPICS.iter().copied().collect();

    for topic in SystemTopic::ALL {
        assert!(
            served.contains(topic) || unserved.contains(topic) || daemons.contains(topic),
            "the ontology declares {topic} and nothing serves it. Add a broker, \
             or add it to UNSERVED_TOPICS — a topic that is silently unserved \
             is a unit that never renders."
        );
    }
}

#[test]
fn every_action_is_served_or_named_unserved() {
    let served = served_actions();
    let unserved: HashSet<ActionKind> = UNSERVED_ACTIONS.iter().copied().collect();
    let daemons: HashSet<ActionKind> = DAEMON_ACTIONS.iter().copied().collect();

    for kind in ActionKind::ALL {
        assert!(
            served.contains(kind) || unserved.contains(kind) || daemons.contains(kind),
            "the ontology declares {} and nothing serves it. Add a broker, or \
             add it to UNSERVED_ACTIONS — an action that is silently unserved \
             is a capability granted for something that never happens.",
            kind.name()
        );
    }
}

#[test]
fn nothing_is_named_unserved_and_then_served() {
    // The lists above are an inventory of holes, not a place to leave a name
    // behind. A broker that lands has to delete its line, or the inventory
    // stops being the truth about what is missing.
    let served = served_topics();
    for topic in UNSERVED_TOPICS.iter().chain(DAEMON_TOPICS) {
        assert!(
            !served.contains(topic),
            "{topic} is served by a broker but still listed in UNSERVED_TOPICS"
        );
    }

    let served = served_actions();
    for kind in UNSERVED_ACTIONS.iter().chain(DAEMON_ACTIONS) {
        assert!(
            !served.contains(kind),
            "{} is served by a broker but still listed as unserved",
            kind.name()
        );
    }
}

#[test]
fn no_two_brokers_claim_the_same_thing() {
    // Two brokers on one topic is two connections racing to last-value-wins,
    // and two on one action is a route that depends on registration order.
    let brokers = Brokers::all();

    let mut topics = HashSet::new();
    for broker in &brokers {
        for topic in broker.topics() {
            assert!(
                topics.insert(*topic),
                "{topic} is claimed by more than one broker"
            );
        }
    }

    let mut actions = HashSet::new();
    for broker in &brokers {
        for kind in broker.actions() {
            assert!(
                actions.insert(*kind),
                "{} is claimed by more than one broker",
                kind.name()
            );
        }
    }
}
