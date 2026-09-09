//! A manifest's strings are addresses on the wire; every one of them is
//! checked before the daemon vouches for the unit.

use omega_proto::Address;
use omega_proto::Validated;
use omega_proto::omega::SurfaceKind;
use omega_proto::{Manifest, Surface};
use omega_proto::{SurfaceId, UnitName};

fn unit(name: &str) -> UnitName {
    UnitName::parse(name).unwrap()
}

fn manifest() -> Manifest {
    Manifest {
        capabilities: vec!["CAPABILITY_STATE_READ".into()],
        surfaces: vec![Surface::new(
            SurfaceId::parse("battery").unwrap(),
            SurfaceKind::Widget,
        )],
        state_topics: vec!["battery".into()],
        events: vec!["EVENT_AC_PLUGGED".into()],
        ..Manifest::new(unit("battery-widget"), "0.1.0")
    }
}

#[test]
fn a_complete_manifest_validates() {
    let manifest = manifest();
    manifest.validate(&unit("battery-widget")).unwrap();

    assert_eq!(
        manifest.state_topics().unwrap(),
        vec![Address::parse("battery").unwrap()]
    );
    assert_eq!(manifest.events().unwrap().len(), 1);
}

#[test]
fn a_typo_in_a_state_topic_is_a_build_error() {
    let manifest = Manifest {
        state_topics: vec!["batery".into()],
        ..manifest()
    };

    let err = manifest.validate(&unit("battery-widget")).unwrap_err();
    assert!(err.to_string().contains("batery"), "{err}");
}

#[test]
fn a_unit_may_declare_a_keyspace_topic() {
    let manifest = Manifest {
        state_topics: vec!["battery".into(), "unit.clock.format".into()],
        ..manifest()
    };
    manifest.validate(&unit("battery-widget")).unwrap();
}

#[test]
fn a_typo_in_an_event_is_a_build_error() {
    let manifest = Manifest {
        events: vec!["EVENT_AC_UNPLUGED".into()],
        ..manifest()
    };

    let err = manifest.validate(&unit("battery-widget")).unwrap_err();
    assert!(err.to_string().contains("EVENT_AC_UNPLUGED"), "{err}");
}
