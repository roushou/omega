//! Topic addresses are validated at the edge.

use omega_wire::{SystemTopic, Topic, TopicError};

#[test]
fn system_topics_are_a_closed_set() {
    assert_eq!(
        Topic::parse("battery").unwrap(),
        Topic::System(SystemTopic::Battery)
    );
    assert_eq!(
        Topic::parse("batery").unwrap_err(),
        TopicError::Unknown("batery".into())
    );
    // Every system topic round-trips through its address.
    for topic in SystemTopic::ALL {
        assert_eq!(Topic::parse(topic.as_str()).unwrap(), Topic::System(*topic));
    }
}

#[test]
fn a_unit_keyspace_address_names_its_owner() {
    let topic = Topic::parse("unit.battery-widget.threshold").unwrap();
    assert_eq!(topic.owner(), Some("battery-widget"));
    assert_eq!(topic.to_string(), "unit.battery-widget.threshold");

    // A system topic has no unit owner: no capability makes it writable.
    assert_eq!(Topic::parse("battery").unwrap().owner(), None);
}

#[test]
fn a_malformed_unit_address_is_rejected() {
    for address in ["unit.", "unit.battery-widget", "unit..key", "unit.name."] {
        assert!(
            matches!(
                Topic::parse(address),
                Err(TopicError::MalformedUnitTopic(_))
            ),
            "{address} should not parse"
        );
    }
}

#[test]
fn keys_may_contain_dots() {
    let topic = Topic::parse("unit.clock.format.long").unwrap();
    assert_eq!(topic, Topic::of_unit("clock", "format.long"));
}
