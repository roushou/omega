//! Plugin lifecycle transition tests.

use omega_daemon::hub::Hub;
use omega_daemon::plugins::{Lifecycle, PluginRegistry, Transition};
use omega_proto::PluginName;
use omega_proto::omega::PluginPhase;

fn plugin(name: &str) -> PluginName {
    PluginName::try_from(name).unwrap()
}

fn table() -> PluginRegistry {
    PluginRegistry::detached(Hub::new())
}

fn phase(plugins: &PluginRegistry, name: &PluginName) -> i32 {
    plugins
        .statuses()
        .into_iter()
        .find(|status| status.plugin == name.as_str())
        .expect("the plugin should be in the table")
        .phase
}

#[test]
fn a_spawned_plugin_is_starting_until_it_checks_in() {
    let plugins = table();
    let name = plugin("battery-widget");
    let token = plugins.issue(&name).unwrap();

    plugins.transition(&name, Transition::Spawned);

    // A spawned process remains starting until admission.
    assert_eq!(phase(&plugins, &name), PluginPhase::Starting as i32);

    // The handshake is what makes it running.
    let (requests, _answers) = tokio::sync::mpsc::channel(1);
    assert_eq!(plugins.identify(4242, token.as_str()), Some(name.clone()));
    let _session = plugins.connected(&name, requests);

    assert_eq!(phase(&plugins, &name), PluginPhase::Running as i32);
}

#[test]
fn a_plugin_that_loses_its_session_is_no_longer_running() {
    let plugins = table();
    let name = plugin("battery-widget");
    plugins.transition(&name, Transition::Spawned);

    let (requests, _answers) = tokio::sync::mpsc::channel(1);
    let session = plugins.connected(&name, requests);
    assert_eq!(phase(&plugins, &name), PluginPhase::Running as i32);

    // The process may still be up, but a plugin the daemon cannot reach is not
    // one it should report as running.
    drop(session);
    assert_eq!(phase(&plugins, &name), PluginPhase::Starting as i32);
    assert!(!plugins.is_connected(&name));
}

#[test]
fn an_exit_carries_its_reason_into_the_status() {
    let plugins = table();
    let name = plugin("battery-widget");

    plugins.transition(&name, Transition::Spawned);
    plugins.transition(
        &name,
        Transition::Exited {
            code: 101,
            detail: "exit status: 101".into(),
        },
    );

    let status = plugins
        .statuses()
        .into_iter()
        .find(|status| status.plugin == name.as_str())
        .unwrap();
    assert_eq!(status.phase, PluginPhase::Restarting as i32);
    assert_eq!(status.last_exit_code, 101);
    assert_eq!(status.detail, "exit status: 101");
}

#[test]
fn restarts_count_spawns_after_the_first() {
    let plugins = table();
    let name = plugin("battery-widget");

    plugins.transition(&name, Transition::Spawned);
    assert_eq!(plugins.statuses()[0].restarts, 0);

    for expected in 1..=3 {
        plugins.transition(
            &name,
            Transition::Exited {
                code: 1,
                detail: "exit status: 1".into(),
            },
        );
        plugins.transition(&name, Transition::Spawned);
        assert_eq!(plugins.statuses()[0].restarts, expected);
    }
}

#[test]
fn a_stopped_plugin_stays_stopped_until_something_spawns_it() {
    let mut lifecycle = Lifecycle::Running;

    assert!(lifecycle.apply(Transition::Stopped));
    assert_eq!(lifecycle, Lifecycle::Stopped);

    // A late exit report from the process that was just killed must not
    // resurrect it as "restarting".
    assert!(!lifecycle.apply(Transition::Exited {
        code: 0,
        detail: String::new(),
    }));
    assert_eq!(lifecycle, Lifecycle::Stopped);

    // Only a new spawn does.
    assert!(lifecycle.apply(Transition::Spawned));
    assert_eq!(lifecycle, Lifecycle::Starting);
}

#[test]
fn a_peer_the_supervisor_never_spawned_has_no_lifecycle_to_change() {
    let mut lifecycle = Lifecycle::Idle;

    // An operator's session, or a plugin registered by hand in a test: being
    // connected does not mean the supervisor is running it.
    assert!(!lifecycle.connected());
    assert_eq!(lifecycle, Lifecycle::Idle);
}

#[tokio::test]
async fn obsolete_session_cannot_disconnect_its_replacement() {
    let plugins = omega_daemon::plugins::PluginRegistry::detached(omega_daemon::hub::Hub::new());
    let name = "plugin".parse::<omega_proto::PluginName>().unwrap();
    let first = plugins.connected(&name, tokio::sync::mpsc::channel(1).0);
    let second = plugins.connected(&name, tokio::sync::mpsc::channel(1).0);
    first.cancelled().await;
    drop(first);
    assert!(plugins.is_connected(&name));
    assert!(second.is_current());
    drop(second);
    assert!(!plugins.is_connected(&name));
}

#[test]
fn obsolete_adoption_cannot_release_its_replacement() {
    let plugins = omega_daemon::plugins::PluginRegistry::detached(omega_daemon::hub::Hub::new());
    let name = "plugin".parse::<omega_proto::PluginName>().unwrap();
    let old = plugins.adopt_plugin(&name).unwrap();
    let current = plugins.adopt_plugin(&name).unwrap();
    plugins.release_adoption(&name, &old);
    assert!(plugins.held().contains(&name));
    plugins.release_adoption(&name, &current);
    assert!(!plugins.held().contains(&name));
}
