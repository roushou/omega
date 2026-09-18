//! Command surfaces: the daemon asking a plugin to do something.

mod common;

use std::time::Duration;

use common::{Harness, command_manifest, expect_refusal, next_result, widget_manifest};
use omega_daemon::manifest::ManifestStore;
use omega_proto::omega::{
    Act, Action, ErrorCode, Frame, Invoke, InvokePlugin, Value, action, frame, invoke, result,
    value,
};
use omega_proto::{IntoValue, Manifest};

fn call(stream_id: u64, plugin: &str, command: &str) -> Frame {
    Frame {
        stream_id,
        body: Some(frame::Body::Invoke(Invoke {
            op: Some(invoke::Op::Act(Act {
                action: Some(Action {
                    kind: Some(action::Kind::InvokePlugin(InvokePlugin {
                        signature: Vec::new(),
                        plugin: plugin.into(),
                        command: command.into(),
                        args: vec![Value {
                            kind: Some(value::Kind::StringValue("now".into())),
                        }],
                    })),
                }),
            })),
        })),
    }
}

async fn connected_plugin(
    harness: &Harness,
    manifest: &Manifest,
) -> omega_proto::Transport<tokio::net::UnixStream> {
    let token = harness.register_plugin(manifest.name.as_str());
    let mut transport = harness.connect(&manifest.hash(), token.as_str()).await;
    transport.recv().await.unwrap().unwrap(); // Welcome
    transport
}

#[tokio::test]
async fn an_operator_calls_a_command_and_gets_its_answer() {
    let manifest = command_manifest("lamp", "toggle");
    let harness = Harness::new(
        "command-call",
        ManifestStore::from_manifests([manifest.clone()]),
    );

    // The plugin that serves the command...
    let mut plugin = connected_plugin(&harness, &manifest).await;

    // ...and the operator asking for it.
    let mut operator = harness.connect("operator", "").await;
    operator.recv().await.unwrap().unwrap(); // Welcome
    operator.send(call(1, "lamp", "toggle")).await.unwrap();

    // The plugin sees the call on one of the daemon's own streams, with the
    // arguments it was given.
    let frame = tokio::time::timeout(Duration::from_secs(2), plugin.recv())
        .await
        .expect("the daemon should forward the call")
        .unwrap()
        .unwrap();
    assert!(frame.stream_id % 2 == 0, "daemon streams are even");

    let Some(frame::Body::Invoke(Invoke {
        op: Some(invoke::Op::CallCommand(called)),
    })) = frame.body
    else {
        panic!("expected a CallCommand");
    };
    assert_eq!(called.command, "toggle");
    // Forward bound arguments without replacing their values.
    assert_eq!(
        called.args,
        vec![Value {
            kind: Some(value::Kind::StringValue("now".into())),
        }]
    );

    // The plugin answers, and the answer reaches the operator.
    plugin
        .send(Frame {
            stream_id: frame.stream_id,
            body: Some(frame::Body::Result(omega_proto::omega::Result {
                outcome: Some(result::Outcome::Value(Value {
                    kind: Some(value::Kind::StringValue("on".into())),
                })),
                done: true,
            })),
        })
        .await
        .unwrap();

    let answer = next_result(&mut operator).await.unwrap();
    assert_eq!(answer.stream_id, 1, "answered on the stream that asked");
    match common::expect_outcome(Some(answer)) {
        result::Outcome::Value(value) => {
            assert_eq!(value.kind, Some(value::Kind::StringValue("on".into())))
        }
        other => panic!("expected the plugin's answer, got {other:?}"),
    }
}

#[tokio::test]
async fn unexpected_command_replies_are_refused_on_the_callers_stream() {
    let manifest = command_manifest("lamp", "toggle");
    let harness = Harness::new(
        "command-answer-kind",
        ManifestStore::from_manifests([manifest.clone()]),
    );
    let mut plugin = connected_plugin(&harness, &manifest).await;
    let mut operator = harness.connect("operator", "").await;
    operator.recv().await.unwrap().unwrap();
    for (index, outcome) in [
        result::Outcome::State(Default::default()),
        result::Outcome::View(Default::default()),
        result::Outcome::Output(Default::default()),
        result::Outcome::Deployment(Default::default()),
        result::Outcome::Instances(Default::default()),
    ]
    .into_iter()
    .enumerate()
    {
        let stream = 1 + 2 * index as u64;
        operator.send(call(stream, "lamp", "toggle")).await.unwrap();
        let request = tokio::time::timeout(Duration::from_secs(2), plugin.recv())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        plugin
            .send(Frame::reply(request.stream_id, outcome))
            .await
            .unwrap();
        let answer = next_result(&mut operator).await.unwrap();
        assert_eq!(answer.stream_id, stream);
        assert_eq!(
            expect_refusal(Some(answer)).code,
            ErrorCode::InvalidArgument
        );
    }
}

#[tokio::test]
async fn a_command_the_plugin_never_declared_is_refused_before_it_is_asked() {
    let manifest = command_manifest("lamp", "toggle");
    let harness = Harness::new(
        "command-undeclared",
        ManifestStore::from_manifests([manifest.clone()]),
    );
    let _plugin = connected_plugin(&harness, &manifest).await;

    let mut operator = harness.connect("operator", "").await;
    operator.recv().await.unwrap().unwrap(); // Welcome
    operator.send(call(1, "lamp", "togle")).await.unwrap();

    // Validate command names against the manifest before routing.
    let refusal = expect_refusal(next_result(&mut operator).await);
    assert_eq!(refusal.code, ErrorCode::InvalidArgument);
    assert!(refusal.message.contains("toggle"), "{refusal}");
}

#[tokio::test]
async fn a_plugin_that_declares_no_commands_says_so() {
    let manifest = widget_manifest("battery-widget", "battery");
    let harness = Harness::new(
        "command-none",
        ManifestStore::from_manifests([manifest.clone()]),
    );
    let _plugin = connected_plugin(&harness, &manifest).await;

    let mut operator = harness.connect("operator", "").await;
    operator.recv().await.unwrap().unwrap(); // Welcome
    operator
        .send(call(1, "battery-widget", "toggle"))
        .await
        .unwrap();

    let refusal = expect_refusal(next_result(&mut operator).await);
    assert!(refusal.message.contains("undeclared command"), "{refusal}");
}

#[tokio::test]
async fn calling_a_plugin_that_is_not_connected_is_refused() {
    let manifest = command_manifest("lamp", "toggle");
    let harness = Harness::new("command-absent", ManifestStore::from_manifests([manifest]));

    let mut operator = harness.connect("operator", "").await;
    operator.recv().await.unwrap().unwrap(); // Welcome
    operator.send(call(1, "lamp", "toggle")).await.unwrap();

    let refusal = expect_refusal(next_result(&mut operator).await);
    // Not a bad request: the caller asked for something reasonable of a plugin
    // that is not there yet.
    assert_eq!(refusal.code, ErrorCode::Unavailable);
    assert!(refusal.message.contains("not connected"), "{refusal}");
}

#[tokio::test]
async fn a_plugins_own_refusal_reaches_the_caller_as_it_was() {
    let manifest = command_manifest("lamp", "toggle");
    let harness = Harness::new(
        "command-refused",
        ManifestStore::from_manifests([manifest.clone()]),
    );
    let mut plugin = connected_plugin(&harness, &manifest).await;

    let mut operator = harness.connect("operator", "").await;
    operator.recv().await.unwrap().unwrap(); // Welcome
    operator.send(call(1, "lamp", "toggle")).await.unwrap();

    let frame = tokio::time::timeout(Duration::from_secs(2), plugin.recv())
        .await
        .expect("the daemon should forward the call")
        .unwrap()
        .unwrap();

    // The plugin says no, in its own words and with its own code.
    plugin
        .send(omega_proto::Refusal::denied("the lamp is bolted to the wall").frame(frame.stream_id))
        .await
        .unwrap();

    // The daemon was the messenger; flattening the plugin's answer into one of
    // its own would lose why.
    let refusal = expect_refusal(next_result(&mut operator).await);
    assert_eq!(refusal.code, ErrorCode::PermissionDenied);
    assert!(refusal.message.contains("bolted to the wall"), "{refusal}");
}

#[tokio::test]
async fn a_plugin_needs_a_declared_dependency_to_invoke_another() {
    let lamp = command_manifest("lamp", "toggle");
    let caller = widget_manifest("battery-widget", "battery");
    let harness = Harness::new(
        "command-plugin-caller",
        ManifestStore::from_manifests([lamp.clone(), caller.clone()]),
    );
    let _lamp = connected_plugin(&harness, &lamp).await;
    let mut caller_plugin = connected_plugin(&harness, &caller).await;

    // Making another plugin run its own code is making code run, and this
    // manifest was never granted that.
    caller_plugin.send(call(3, "lamp", "toggle")).await.unwrap();

    let refusal = expect_refusal(next_result(&mut caller_plugin).await);
    assert_eq!(refusal.code, ErrorCode::PermissionDenied);
    assert!(
        refusal.message.contains("command access not declared"),
        "{refusal}"
    );
}

#[tokio::test]
async fn a_plugin_can_receive_its_own_command_while_waiting_for_the_answer() {
    let manifest = command_manifest("lamp", "toggle").granting([
        omega_proto::omega::Capability::StateRead,
        omega_proto::omega::Capability::Spawn,
    ]);
    let harness = Harness::new(
        "self-call",
        ManifestStore::from_manifests([manifest.clone()]),
    );
    let mut plugin = connected_plugin(&harness, &manifest).await;
    plugin.send(call(1, "lamp", "toggle")).await.unwrap();
    let request = tokio::time::timeout(Duration::from_secs(1), plugin.recv())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(matches!(request.body, Some(frame::Body::Invoke(_))));
    plugin
        .send(Frame {
            stream_id: request.stream_id,
            body: Some(frame::Body::Result(omega_proto::omega::Result {
                outcome: Some(result::Outcome::Ok(Default::default())),
                done: true,
            })),
        })
        .await
        .unwrap();
    let answer = next_result(&mut plugin).await.unwrap();
    assert_eq!(answer.stream_id, 1);
    common::expect_ok(Some(answer));
}

struct PendingCalls;
impl PendingCalls {
    fn op(bytes: usize) -> invoke::Op {
        invoke::Op::CallCommand(omega_proto::omega::CallCommand {
            command: "toggle".into(),
            args: vec![Value {
                kind: Some(value::Kind::StringValue("x".repeat(bytes))),
            }],
        })
    }

    fn start(
        harness: &Harness,
        bytes: usize,
    ) -> tokio::task::JoinHandle<Result<result::Outcome, omega_daemon::plugins::RequestError>> {
        let plugins = harness.plugins.clone();
        tokio::spawn(async move {
            plugins
                .request(
                    &"lamp".parse::<omega_proto::PluginName>().unwrap(),
                    Self::op(bytes),
                )
                .await
        })
    }

    async fn barrier(plugin: &mut omega_proto::Transport<tokio::net::UnixStream>) {
        plugin
            .send(Frame {
                stream_id: 1,
                body: Some(frame::Body::Ping(Default::default())),
            })
            .await
            .unwrap();
        assert!(matches!(
            plugin.recv().await.unwrap().unwrap().body,
            Some(frame::Body::Pong(_))
        ));
    }
}

#[tokio::test]
async fn abandoned_wire_requests_keep_byte_capacity_until_terminal_reply_or_disconnect() {
    let manifest = command_manifest("lamp", "toggle");
    let harness = Harness::new(
        "pending-bytes",
        ManifestStore::from_manifests([manifest.clone()]),
    );
    let mut plugin = connected_plugin(&harness, &manifest).await;
    let mut streams = Vec::new();
    for _ in 0..2 {
        let call = PendingCalls::start(&harness, 3 * 1024 * 1024);
        streams.push(plugin.recv().await.unwrap().unwrap().stream_id);
        call.abort();
        assert!(call.await.unwrap_err().is_cancelled());
    }
    assert!(matches!(
        PendingCalls::start(&harness, 3 * 1024 * 1024)
            .await
            .unwrap(),
        Err(omega_daemon::plugins::RequestError::Full(_))
    ));
    plugin
        .send(Frame::reply(
            streams[0],
            result::Outcome::Ok(Default::default()),
        ))
        .await
        .unwrap();
    PendingCalls::barrier(&mut plugin).await;
    let admitted = PendingCalls::start(&harness, 3 * 1024 * 1024);
    assert!(matches!(
        plugin.recv().await.unwrap().unwrap().body,
        Some(frame::Body::Invoke(_))
    ));
    drop(plugin);
    assert!(matches!(
        admitted.await.unwrap(),
        Err(omega_daemon::plugins::RequestError::Absent(_))
    ));
    let mut plugin = connected_plugin(&harness, &manifest).await;
    for _ in 0..2 {
        let call = PendingCalls::start(&harness, 3 * 1024 * 1024);
        assert!(matches!(
            plugin.recv().await.unwrap().unwrap().body,
            Some(frame::Body::Invoke(_))
        ));
        call.abort();
        assert!(call.await.unwrap_err().is_cancelled());
    }
}

#[tokio::test]
async fn streamed_results_and_cancelled_callers_do_not_release_pending_slots_early() {
    let manifest = command_manifest("lamp", "toggle");
    let harness = Harness::new(
        "pending-count",
        ManifestStore::from_manifests([manifest.clone()]),
    );
    let mut plugin = connected_plugin(&harness, &manifest).await;
    let first = PendingCalls::start(&harness, 0);
    let stream = plugin.recv().await.unwrap().unwrap().stream_id;
    plugin
        .send(Frame {
            stream_id: stream,
            body: Some(frame::Body::Result(omega_proto::omega::Result {
                done: false,
                outcome: Some(result::Outcome::Value(Default::default())),
            })),
        })
        .await
        .unwrap();
    PendingCalls::barrier(&mut plugin).await;
    assert!(!first.is_finished());
    first.abort();
    assert!(first.await.unwrap_err().is_cancelled());
    for _ in 1..16 {
        let call = PendingCalls::start(&harness, 0);
        assert!(matches!(
            plugin.recv().await.unwrap().unwrap().body,
            Some(frame::Body::Invoke(_))
        ));
        call.abort();
        assert!(call.await.unwrap_err().is_cancelled());
    }
    assert!(
        matches!(PendingCalls::start(&harness, 0).await.unwrap(), Err(omega_daemon::plugins::RequestError::Refused { source, .. }) if source.code == ErrorCode::ResourceExhausted)
    );
    plugin
        .send(Frame::reply(
            stream,
            result::Outcome::Ok(Default::default()),
        ))
        .await
        .unwrap();
    PendingCalls::barrier(&mut plugin).await;
    let admitted = PendingCalls::start(&harness, 0);
    let stream = plugin.recv().await.unwrap().unwrap().stream_id;
    plugin
        .send(Frame::reply(
            stream,
            result::Outcome::Ok(Default::default()),
        ))
        .await
        .unwrap();
    assert!(admitted.await.unwrap().is_ok());
}

#[tokio::test]
async fn a_declared_dependency_routes_without_spawn_and_discovery_does_not_widen_access() {
    let target = command_manifest("target", "set");
    let hidden = command_manifest("hidden", "set");
    let mut caller_manifest = widget_manifest("caller", "panel");
    caller_manifest
        .command_dependencies
        .push(target.commands[0].dependency("target"));
    let harness = Harness::new(
        "typed-call",
        ManifestStore::from_manifests([target.clone(), hidden, caller_manifest.clone()]),
    );
    let mut target_peer = connected_plugin(&harness, &target).await;
    let mut caller = connected_plugin(&harness, &caller_manifest).await;
    caller
        .send(Frame {
            stream_id: 1,
            body: Some(frame::Body::Invoke(Invoke {
                op: Some(invoke::Op::ListCommands(Default::default())),
            })),
        })
        .await
        .unwrap();
    let result::Outcome::Commands(catalogue) =
        common::expect_outcome(next_result(&mut caller).await)
    else {
        panic!("catalogue expected");
    };
    assert_eq!(catalogue.entries.len(), 1);
    assert_eq!(catalogue.entries[0].plugin, "target");
    assert!(catalogue.entries[0].available);
    let mut request = call(3, "target", "set");
    if let Some(frame::Body::Invoke(Invoke {
        op: Some(invoke::Op::Act(act)),
    })) = request.body.as_mut()
        && let Some(action::Kind::InvokePlugin(call)) =
            act.action.as_mut().and_then(|a| a.kind.as_mut())
    {
        call.signature = target.commands[0].signature("target");
    }
    caller.send(request).await.unwrap();
    let request = target_peer.recv().await.unwrap().unwrap();
    target_peer
        .send(Frame::reply(
            request.stream_id,
            result::Outcome::Value("typed result".into_value()),
        ))
        .await
        .unwrap();
    assert_eq!(
        common::expect_outcome(next_result(&mut caller).await),
        result::Outcome::Value("typed result".into_value())
    );
    caller.send(call(5, "hidden", "set")).await.unwrap();
    assert_eq!(
        expect_refusal(next_result(&mut caller).await).code,
        ErrorCode::PermissionDenied
    );
}
