//! Topic addresses are validated at the edge.

use std::collections::HashSet;

use omega_proto::{Address, AddressError, SystemTopic};

#[test]
fn system_topics_are_a_closed_set() {
    assert_eq!(
        Address::parse("battery").unwrap(),
        Address::System(SystemTopic::Battery)
    );
    assert_eq!(
        Address::parse("batery").unwrap_err(),
        AddressError::Unknown("batery".into())
    );
    // Every system topic round-trips through its address.
    for topic in SystemTopic::ALL {
        assert_eq!(
            Address::parse(topic.as_str()).unwrap(),
            Address::System(*topic)
        );
    }
}

#[test]
fn a_unit_keyspace_address_names_its_owner() {
    let topic = Address::parse("unit.battery-widget.threshold").unwrap();
    assert_eq!(topic.owner(), Some("battery-widget"));
    assert_eq!(topic.to_string(), "unit.battery-widget.threshold");

    // A system topic has no unit owner: no capability makes it writable.
    assert_eq!(Address::parse("battery").unwrap().owner(), None);
}

#[test]
fn a_malformed_unit_address_is_rejected() {
    for address in ["unit.", "unit.battery-widget", "unit..key", "unit.name."] {
        assert!(
            matches!(
                Address::parse(address),
                Err(AddressError::MalformedUnitTopic(_))
            ),
            "{address} should not parse"
        );
    }
}

#[test]
fn keys_may_contain_dots() {
    let topic = Address::parse("unit.clock.format.long").unwrap();
    assert_eq!(topic, Address::of_unit("clock", "format.long"));
}

#[test]
fn every_topic_is_addressed_once() {
    // The table generates the enum, so `ALL` cannot miss a topic. Two rows
    // sharing an address is the one collision it cannot catch, and a
    // duplicate would make one of them unreachable through `parse`.
    let names: HashSet<&str> = SystemTopic::ALL.iter().map(|t| t.as_str()).collect();
    assert_eq!(names.len(), SystemTopic::ALL.len());
}
