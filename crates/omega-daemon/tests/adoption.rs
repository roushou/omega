//! Taking a plugin's place: what `omega dev` asks the daemon for.

mod common;

use std::time::Duration;

use common::{Harness, expect_outcome, expect_refusal, next_result, plugin_name, widget_manifest};
use omega_daemon::manifest::ManifestStore;
use omega_daemon::plugins::PluginRecord;
use omega_proto::omega::{
    AdoptPlugin, ErrorCode, Frame, Invoke, PluginPhase, Welcome, frame, invoke, result, value,
};
use omega_proto::{Refusal, Transport};

fn adopt(stream_id: u64, plugin: &str) -> Frame {
    Frame {
        stream_id,
        body: Some(frame::Body::Invoke(Invoke {
            op: Some(invoke::Op::AdoptPlugin(AdoptPlugin {
                plugin: plugin.into(),
            })),
        })),
    }
}

fn harness(tag: &str) -> Harness {
    Harness::new(
        tag,
        ManifestStore::from_manifests([widget_manifest("battery-widget", "battery")]),
    )
}

/// Ask as the operator, and take the token back.
async fn adopted(transport: &mut Transport<tokio::net::UnixStream>, plugin: &str) -> String {
    transport.send(adopt(1, plugin)).await.unwrap();
    match expect_outcome(next_result(transport).await) {
        result::Outcome::Value(value) => match value.kind {
            Some(value::Kind::StringValue(token)) => token,
            other => panic!("AdoptPlugin answered with {other:?}"),
        },
        other => panic!("AdoptPlugin answered with {other:?}"),
    }
}

fn welcome(frame: Option<Frame>) -> Welcome {
    match frame.expect("expected a Welcome, got EOF").body {
        Some(frame::Body::Welcome(welcome)) => welcome,
        other => panic!("expected a Welcome, got {other:?}"),
    }
}

/// Poll until the asynchronous adoption cleanup completes.
async fn until(within: Duration, done: impl Fn() -> bool) {
    let deadline = tokio::time::Instant::now() + within;
    while tokio::time::Instant::now() < deadline {
        if done() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("the daemon did not get there within {within:?}");
}

#[tokio::test]
async fn a_token_the_daemon_hands_out_makes_this_process_the_plugin() {
    let harness = harness("adopt-identity");
    let name = plugin_name("battery-widget");

    let mut operator = harness.connect("operator", "").await;
    operator.recv().await.unwrap().unwrap(); // Welcome

    let token = adopted(&mut operator, "battery-widget").await;

    // The token is what a spawned plugin would have been given, and it admits
    // this process as that plugin — with the manifest's grants, not more.
    let hash = widget_manifest("battery-widget", "battery").hash();
    let mut plugin = harness.connect(&hash, &token).await;
    let welcome = welcome(plugin.recv().await.unwrap());

    assert_eq!(welcome.plugin_id, name.to_string());
    assert!(
        !welcome.capabilities.is_empty(),
        "an adopted plugin is granted what its manifest declares"
    );

    // Adopted lifecycle reports the replacement process.
    until(Duration::from_secs(2), || {
        phase_of(&harness, &name) == PluginPhase::Running as i32
    })
    .await;
}

fn phase_of(harness: &Harness, name: &omega_proto::PluginName) -> i32 {
    harness
        .plugins
        .statuses()
        .into_iter()
        .find(|status| status.plugin == name.to_string())
        .map(|status| status.phase)
        .unwrap_or_default()
}

#[tokio::test]
async fn an_adopted_plugin_is_not_started_underneath_the_process_developing_it() {
    let harness = harness("adopt-held");
    let name = plugin_name("battery-widget");

    let mut operator = harness.connect("operator", "").await;
    operator.recv().await.unwrap().unwrap();
    adopted(&mut operator, "battery-widget").await;

    // Active adoption prevents convergence from spawning another process.
    assert!(
        harness.supervisor.running().contains(&name),
        "an adopted plugin must count as held, or the built binary races the one being written"
    );

    // Status must identify the adopted process.
    let status = harness
        .plugins
        .statuses()
        .into_iter()
        .find(|status| status.plugin == name.to_string())
        .expect("the plugin is in the table");
    assert_eq!(status.detail, PluginRecord::ADOPTED);
}

#[tokio::test]
async fn an_adoption_ends_with_the_connection_that_asked_for_it() {
    let harness = harness("adopt-release");
    let name = plugin_name("battery-widget");

    let mut operator = harness.connect("operator", "").await;
    operator.recv().await.unwrap().unwrap();
    let token = adopted(&mut operator, "battery-widget").await;

    // Closing the terminal is how a dev session usually ends, and the plugin
    // has to come back from it.
    drop(operator);
    until(Duration::from_secs(2), || {
        !harness.supervisor.running().contains(&name)
    })
    .await;

    // An expired adoption token grants no plugin authority.
    let hash = widget_manifest("battery-widget", "battery").hash();
    let mut late = harness.connect(&hash, &token).await;
    let refusal = expect_refusal(late.recv().await.unwrap());
    assert_eq!(refusal.code, ErrorCode::Unauthenticated);
}

#[tokio::test]
async fn a_plugin_may_not_take_another_plugins_place() {
    let harness = harness("adopt-denied");
    let token = harness.register_plugin("battery-widget");
    let hash = widget_manifest("battery-widget", "battery").hash();

    let mut plugin = harness.connect(&hash, token.as_str()).await;
    plugin.recv().await.unwrap().unwrap(); // Welcome

    plugin.send(adopt(1, "battery-widget")).await.unwrap();
    let refusal: Refusal = expect_refusal(next_result(&mut plugin).await);

    // Handing out identity is the owner's business. A plugin that could adopt
    // its neighbour could become it.
    assert_eq!(refusal.code, ErrorCode::PermissionDenied);
}

#[tokio::test]
async fn adopting_something_this_build_never_produced_is_refused() {
    let harness = harness("adopt-unknown");

    let mut operator = harness.connect("operator", "").await;
    operator.recv().await.unwrap().unwrap();

    operator.send(adopt(1, "clock")).await.unwrap();
    let refusal = expect_refusal(next_result(&mut operator).await);

    // A token for a name with no manifest is a token the handshake would
    // refuse a moment later; refusing it here says why.
    assert_eq!(refusal.code, ErrorCode::FailedPrecondition);
    assert!(refusal.message.contains("clock"), "{refusal}");
}
