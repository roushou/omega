//! A manifest's strings are addresses on the wire; every one of them is
//! checked before the daemon vouches for the unit.
//!
//! Capabilities, surface kinds and events are enums now, so a *typo* in one
//! is not expressible — the build would not compile. What is still
//! expressible is a value this build cannot name, which is what a manifest
//! from a newer plugin decodes to, and that must be refused rather than
//! dropped.

use omega_proto::omega::{Capability, EventKind, SurfaceKind};
use omega_proto::{Address, Manifest, Surface};
use omega_proto::{SurfaceId, UnitName};

fn unit(name: &str) -> UnitName {
    UnitName::parse(name).unwrap()
}

fn manifest() -> Manifest {
    Manifest::new(&unit("battery-widget"), "0.1.0")
        .granting([Capability::StateRead])
        .exposing([Surface::new(
            &SurfaceId::parse("battery").unwrap(),
            SurfaceKind::Widget,
        )])
        .reading(["battery"])
        .handling([EventKind::EventAcPlugged])
}

#[test]
fn a_complete_manifest_validates() {
    let manifest = manifest();
    manifest.validate(&unit("battery-widget")).unwrap();

    assert_eq!(
        manifest.addresses().unwrap(),
        vec![Address::parse("battery").unwrap()]
    );
    assert_eq!(
        manifest.event_kinds().unwrap(),
        vec![EventKind::EventAcPlugged]
    );
}

#[test]
fn a_typo_in_a_state_topic_is_a_build_error() {
    let manifest = manifest().reading(["batery"]);

    let err = manifest.validate(&unit("battery-widget")).unwrap_err();
    assert!(err.to_string().contains("batery"), "{err}");
}

#[test]
fn a_unit_may_declare_a_keyspace_topic() {
    let manifest = manifest().reading(["battery", "unit.clock.format"]);
    manifest.validate(&unit("battery-widget")).unwrap();
}

#[test]
fn an_event_this_build_cannot_name_is_refused() {
    // What a manifest built against a newer ontology decodes to here. It is
    // not an event we can subscribe to, and letting it through would mean a
    // unit silently handling fewer events than it declared.
    let mut manifest = manifest();
    manifest.events = vec![9_999];

    let err = manifest.validate(&unit("battery-widget")).unwrap_err();
    assert!(err.to_string().contains("9999"), "{err}");
}

#[test]
fn a_capability_this_build_cannot_name_is_refused() {
    // The same rule where it matters most: an unnamed grant is one nobody
    // can reason about, and dropping it is how a unit ends up running with
    // less than its manifest says.
    let mut manifest = manifest();
    manifest.capabilities = vec![9_999];

    let err = manifest.validate(&unit("battery-widget")).unwrap_err();
    assert!(err.to_string().contains("9999"), "{err}");
}

#[test]
fn an_unset_surface_kind_is_refused() {
    // The proto zero. A surface whose kind never got set is not a widget by
    // default; it is a manifest that did not say.
    let mut manifest = manifest();
    manifest.surfaces = vec![Surface {
        id: "battery".into(),
        kind: SurfaceKind::Unspecified as i32,
    }];

    assert!(manifest.validate(&unit("battery-widget")).is_err());
}

#[test]
fn the_hash_is_the_declaration_not_its_order() {
    // Two plugins that declare the same things in a different order are the
    // same plugin, and must present the same hash — otherwise the order a
    // macro happened to visit fields in would decide whether a unit can
    // connect.
    let one = manifest().granting([Capability::StateRead, Capability::Spawn]);
    let other = manifest().granting([Capability::Spawn, Capability::StateRead]);

    assert_eq!(one.hash(), other.hash());
}

#[test]
fn a_declaration_that_differs_hashes_differently() {
    let plain = manifest();
    let more = manifest().granting([Capability::StateRead, Capability::Spawn]);

    assert_ne!(plain.hash(), more.hash());
}

#[test]
fn the_canonical_bytes_round_trip() {
    // What the build stages and what the daemon decodes are the same bytes,
    // so decoding them must give back a manifest with the same hash.
    let manifest = manifest();
    let decoded = Manifest::decode_bytes(&manifest.canonical()).unwrap();

    assert_eq!(decoded.hash(), manifest.hash());
    decoded.validate(&unit("battery-widget")).unwrap();
}

#[test]
fn a_manifest_that_names_another_unit_is_refused() {
    let err = manifest().validate(&unit("clock")).unwrap_err();
    assert!(err.to_string().contains("battery-widget"), "{err}");
}
