//! Require every topic and action to have a handler or an explicit unsupported entry.

use std::collections::HashSet;

use omega_platform::Brokers;
use omega_proto::{ActionKind, SystemTopic};

/// Topics no broker projects yet.
const UNSERVED_TOPICS: &[SystemTopic] = &[];

/// Topics projected by the daemon itself.
const DAEMON_TOPICS: &[SystemTopic] = &[SystemTopic::Plugins];

/// Actions without current handlers. Daemon-owned actions are listed separately.
const UNSERVED_ACTIONS: &[ActionKind] = &[];

/// Actions handled by the daemon.
const DAEMON_ACTIONS: &[ActionKind] = &[
    ActionKind::RunCommand,
    ActionKind::CaptureCommand,
    ActionKind::InvokePlugin,
];

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
             is a plugin that never renders."
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
    // Remove implemented brokers from the uncovered inventory.
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
