//! The identity rules a session rests on.

use omega_daemon::hub::Hub;
use omega_daemon::plugins::PluginRegistry;
use omega_proto::PluginName;

fn plugin(name: &str) -> PluginName {
    PluginName::try_from(name).unwrap()
}

fn table() -> PluginRegistry {
    PluginRegistry::detached(Hub::new())
}

#[test]
fn a_token_binds_to_the_first_process_that_uses_it() {
    let plugins = table();
    let token = plugins.issue(&plugin("battery-widget")).unwrap();

    assert_eq!(
        plugins.identify(100, token.as_str()),
        Some(plugin("battery-widget"))
    );
    // Same process, again: still the same plugin.
    assert_eq!(
        plugins.identify(100, token.as_str()),
        Some(plugin("battery-widget"))
    );
    // Anyone else presenting it is not that plugin.
    assert_eq!(plugins.identify(101, token.as_str()), None);
}

#[test]
fn a_revoked_token_is_never_honoured_again() {
    let plugins = table();
    let token = plugins.issue(&plugin("battery-widget")).unwrap();
    plugins.bind(&plugin("battery-widget"), 100);
    plugins.revoke(&plugin("battery-widget"));

    // A reused pid must not inherit a prior process token.
    assert_eq!(plugins.identify(100, token.as_str()), None);
}

#[test]
fn an_unknown_token_identifies_nobody() {
    let plugins = table();
    plugins.issue(&plugin("battery-widget")).unwrap();

    assert_eq!(plugins.identify(100, ""), None);
    assert_eq!(plugins.identify(100, "not-a-token"), None);
}

#[test]
fn every_spawn_gets_its_own_token() {
    let plugins = table();
    let first = plugins.issue(&plugin("battery-widget")).unwrap();
    let second = plugins.issue(&plugin("battery-widget")).unwrap();

    assert_ne!(first.as_str(), second.as_str());
    assert_eq!(first.as_str().len(), 32, "128 bits of hex");

    // The plugin holds one token at a time: the previous spawn's is gone.
    assert_eq!(plugins.identify(100, first.as_str()), None);
    assert_eq!(
        plugins.identify(100, second.as_str()),
        Some(plugin("battery-widget"))
    );
}
