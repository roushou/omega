//! The identity rules a session rests on.

use omega_core::UnitName;
use omega_daemon::hub::Hub;
use omega_daemon::units::UnitTable;

fn unit(name: &str) -> UnitName {
    UnitName::parse(name).unwrap()
}

fn table() -> UnitTable {
    UnitTable::detached(Hub::new())
}

#[test]
fn a_token_binds_to_the_first_process_that_uses_it() {
    let units = table();
    let token = units.issue(&unit("battery-widget"));

    assert_eq!(
        units.identify(100, token.as_str()),
        Some(unit("battery-widget"))
    );
    // Same process, again: still the same unit.
    assert_eq!(
        units.identify(100, token.as_str()),
        Some(unit("battery-widget"))
    );
    // Anyone else presenting it is not that unit.
    assert_eq!(units.identify(101, token.as_str()), None);
}

#[test]
fn a_revoked_token_is_never_honoured_again() {
    let units = table();
    let token = units.issue(&unit("battery-widget"));
    units.bind(&unit("battery-widget"), 100);
    units.revoke(&unit("battery-widget"));

    // This is the pid-reuse case: the unit died, the kernel handed 100 to
    // someone else, and the token is the reason they cannot inherit its
    // grants.
    assert_eq!(units.identify(100, token.as_str()), None);
}

#[test]
fn an_unknown_token_identifies_nobody() {
    let units = table();
    units.issue(&unit("battery-widget"));

    assert_eq!(units.identify(100, ""), None);
    assert_eq!(units.identify(100, "not-a-token"), None);
}

#[test]
fn every_spawn_gets_its_own_token() {
    let units = table();
    let first = units.issue(&unit("battery-widget"));
    let second = units.issue(&unit("battery-widget"));

    assert_ne!(first.as_str(), second.as_str());
    assert_eq!(first.as_str().len(), 32, "128 bits of hex");

    // The unit holds one token at a time: the previous spawn's is gone.
    assert_eq!(units.identify(100, first.as_str()), None);
    assert_eq!(
        units.identify(100, second.as_str()),
        Some(unit("battery-widget"))
    );
}
