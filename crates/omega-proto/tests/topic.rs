//! Topic addresses are validated at the edge.

use std::collections::HashSet;

use omega_proto::{Address, AddressError, SystemTopic};

#[test]
fn system_topics_are_a_closed_set() {
    assert_eq!(
        "battery".parse::<Address>().unwrap(),
        Address::System(SystemTopic::Battery)
    );
    assert_eq!(
        "batery".parse::<Address>().unwrap_err(),
        AddressError::Unknown("batery".into())
    );
    // Every system topic round-trips through its address.
    for topic in SystemTopic::ALL {
        assert_eq!(
            topic.as_str().parse::<Address>().unwrap(),
            Address::System(*topic)
        );
    }
}

#[test]
fn a_unit_keyspace_address_names_its_owner() {
    let topic = "unit.battery-widget.threshold".parse::<Address>().unwrap();
    assert_eq!(topic.owner(), Some("battery-widget"));
    assert_eq!(topic.to_string(), "unit.battery-widget.threshold");

    // A system topic has no unit owner: no capability makes it writable.
    assert_eq!("battery".parse::<Address>().unwrap().owner(), None);
}

#[test]
fn a_malformed_unit_address_is_rejected() {
    for address in ["unit.", "unit.battery-widget", "unit..key", "unit.name."] {
        assert!(
            matches!(
                address.parse::<Address>(),
                Err(AddressError::MalformedUnitTopic(_))
            ),
            "{address} should not parse"
        );
    }
}

#[test]
fn keys_may_contain_dots() {
    let topic = "unit.clock.format.long".parse::<Address>().unwrap();
    assert_eq!(topic, Address::of_unit("clock", "format.long"));
}

#[test]
fn every_topic_is_addressed_once() {
    // Topic addresses must be unique for reverse lookup.
    let names: HashSet<&str> = SystemTopic::ALL.iter().map(|t| t.as_str()).collect();
    assert_eq!(names.len(), SystemTopic::ALL.len());
}
