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
fn a_plugin_keyspace_address_names_its_owner() {
    let topic = "plugin.battery-widget.threshold"
        .parse::<Address>()
        .unwrap();
    assert_eq!(topic.owner(), Some("battery-widget"));
    assert_eq!(topic.to_string(), "plugin.battery-widget.threshold");

    // A system topic has no plugin owner: no capability makes it writable.
    assert_eq!("battery".parse::<Address>().unwrap().owner(), None);
}

#[test]
fn a_malformed_plugin_address_is_rejected() {
    for address in [
        "plugin.",
        "plugin.battery-widget",
        "plugin..key",
        "plugin.name.",
    ] {
        assert!(
            matches!(
                address.parse::<Address>(),
                Err(AddressError::MalformedPluginTopic(_))
            ),
            "{address} should not parse"
        );
    }
}

#[test]
fn keys_may_contain_dots() {
    let topic = "plugin.clock.format.long".parse::<Address>().unwrap();
    assert_eq!(topic, Address::of_plugin("clock", "format.long"));
}

#[test]
fn every_topic_is_addressed_once() {
    // Topic addresses must be unique for reverse lookup.
    let names: HashSet<&str> = SystemTopic::ALL.iter().map(|t| t.as_str()).collect();
    assert_eq!(names.len(), SystemTopic::ALL.len());
}
