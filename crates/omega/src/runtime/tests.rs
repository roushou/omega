use super::*;
use crate::surface::{Widget, Wired};
use crate::ui::{Text, Ui};
use omega_proto::omega::Result as OpResult;
use omega_proto::omega::{
    BatteryState, Capability, NetworkState, RemoveWidget, RenderWidget, StatePatch, StateSnapshot,
    StateTopic, ViewTree, Welcome, state_topic,
};
use omega_proto::{DaemonStreams, SystemTopic, Transport};
use std::time::Duration;

struct Probe<const KIND: u8> {
    context: Context,
    label: String,
}
impl<const KIND: u8> Wired for Probe<KIND> {
    fn topics() -> Vec<SystemTopic> {
        match KIND {
            0 => vec![SystemTopic::Battery],
            1 => vec![SystemTopic::Network],
            2 => vec![SystemTopic::Battery, SystemTopic::Network],
            _ => vec![],
        }
    }
    fn capabilities() -> Vec<Capability> {
        vec![]
    }
    fn keyspaces() -> Vec<String> {
        vec!["unit.other.value".into()]
    }
    fn build(context: &Context, settings: &Values) -> Self {
        Self {
            context: context.clone(),
            label: settings.get("label").unwrap_or_default(),
        }
    }
}
impl<const KIND: u8> Widget for Probe<KIND> {
    fn render(&self) -> Ui {
        assert!(
            self.context.holds(&Self::topics()),
            "rendered before its inputs arrived"
        );
        if self.label == "oversized" {
            return Text::new("x".repeat(omega_proto::MAX_FRAME_LEN)).into();
        }
        if KIND == 3 {
            return Text::new("constant").into();
        }
        match self.context.topic::<BatteryState>() {
            Some(battery) => Text::new(format!("{}{}", self.label, battery.level)).into(),
            None => Ui::empty(),
        }
    }
}

struct Peer {
    transport: Transport<UnixStream>,
    streams: DaemonStreams,
    task: tokio::task::JoinHandle<Result<(), Error>>,
}
impl Peer {
    async fn start(plugin: Plugin, topics: Vec<StateTopic>) -> Self {
        let manifest = plugin.manifest().unwrap();
        let (daemon, unit) = UnixStream::pair().unwrap();
        let task =
            tokio::spawn(async move { Runtime::over(unit, &manifest).await?.serve(plugin).await });
        let mut peer = Self {
            transport: Transport::new(daemon),
            streams: DaemonStreams::new(),
            task,
        };
        assert!(matches!(
            peer.next().await.body,
            Some(frame::Body::Hello(_))
        ));
        peer.send(Frame {
            stream_id: 0,
            body: Some(frame::Body::Welcome(Welcome {
                protocol_version: omega_proto::PROTOCOL_VERSION,
                state: Some(StateSnapshot { topics }),
                ..Default::default()
            })),
        })
        .await;
        peer
    }
    fn battery(revision: u64, level: Option<f64>) -> StateTopic {
        StateTopic {
            topic: "battery".into(),
            revision,
            value: level.map(|level| {
                state_topic::Value::Battery(BatteryState {
                    level,
                    ..Default::default()
                })
            }),
        }
    }
    fn network() -> StateTopic {
        StateTopic {
            topic: "network".into(),
            revision: 1,
            value: Some(state_topic::Value::Network(NetworkState::default())),
        }
    }
    async fn send(&mut self, frame: Frame) {
        self.transport.send(frame).await.unwrap();
    }
    async fn next(&mut self) -> Frame {
        tokio::time::timeout(Duration::from_secs(3), self.transport.recv())
            .await
            .expect("runtime did not answer")
            .unwrap()
            .expect("runtime exited")
    }
    async fn patch(&mut self, topics: Vec<StateTopic>) {
        self.send(Frame {
            stream_id: 0,
            body: Some(frame::Body::StatePatch(StatePatch { topics })),
        })
        .await;
    }
    async fn invoke(&mut self, op: invoke::Op) -> result::Outcome {
        let stream = self.streams.allocate();
        self.send(Frame {
            stream_id: stream,
            body: Some(frame::Body::Invoke(Invoke { op: Some(op) })),
        })
        .await;
        let frame = self.next().await;
        assert_eq!(frame.stream_id, stream, "unexpected publication: {frame:?}");
        let Some(frame::Body::Result(answer)) = frame.body else {
            panic!("expected an answer")
        };
        assert!(answer.done);
        answer.outcome.unwrap()
    }
    // Ordered requests prove preceding patches produced no output, without sleeping.
    async fn quiet(&mut self) {
        let outcome = self.invoke(invoke::Op::GetState(Default::default())).await;
        assert!(matches!(outcome, result::Outcome::Error(_)));
    }
    async fn published(&mut self, surface: &str, module: &str) -> ViewTree {
        let frame = self.next().await;
        let Some(frame::Body::Invoke(Invoke {
            op: Some(invoke::Op::PublishView(view)),
        })) = frame.body
        else {
            panic!("expected a published view")
        };
        assert_eq!(
            (view.surface_id.as_str(), view.module_id.as_str()),
            (surface, module)
        );
        view.view.unwrap()
    }
    async fn render(&mut self, surface: &str, module: &str, label: &str) -> ViewTree {
        let outcome = self
            .invoke(invoke::Op::RenderWidget(RenderWidget {
                surface_id: surface.into(),
                module_id: module.into(),
                config: Values::new().with("label", label).into_map(),
            }))
            .await;
        let result::Outcome::View(view) = outcome else {
            panic!("expected a view")
        };
        view
    }
    fn text(tree: &ViewTree) -> String {
        let root = tree.root.as_ref().expect("expected text");
        omega_proto::FromValue::from_value(root.props.get("text").unwrap()).unwrap()
    }
}
impl Drop for Peer {
    fn drop(&mut self) {
        self.task.abort();
    }
}

#[tokio::test]
async fn surfaces_wait_only_for_their_own_topics_and_records_have_defaults() {
    let plugin = Plugin::named("test", "0.1.0")
        .widget_as::<Probe<0>>("battery")
        .widget_as::<Probe<1>>("network")
        .widget_as::<Probe<2>>("both")
        .widget_as::<Probe<3>>("constant");
    let mut peer = Peer::start(plugin, vec![Peer::battery(1, Some(0.5))]).await;
    assert_eq!(Peer::text(&peer.published("battery", "").await), "0.5");
    assert_eq!(
        Peer::text(&peer.published("constant", "").await),
        "constant"
    );
    peer.quiet().await;
    peer.patch(vec![Peer::network()]).await;
    assert_eq!(Peer::text(&peer.published("network", "").await), "0.5");
    assert_eq!(Peer::text(&peer.published("both", "").await), "0.5");
    peer.quiet().await;
}

#[tokio::test]
async fn requested_instances_wait_without_blocking_and_keep_their_settings() {
    let plugin = Plugin::named("test", "0.1.0").widget_as::<Probe<0>>("battery");
    let mut peer = Peer::start(plugin, vec![]).await;
    assert!(
        peer.render("battery", "bar-1", "placed:")
            .await
            .root
            .is_none()
    );
    peer.quiet().await;
    peer.patch(vec![Peer::battery(1, Some(0.5))]).await;
    assert_eq!(Peer::text(&peer.published("battery", "").await), "0.5");
    assert_eq!(
        Peer::text(&peer.published("battery", "bar-1").await),
        "placed:0.5"
    );
    peer.quiet().await;
}

#[tokio::test]
async fn deduplication_includes_pull_responses_and_retractions() {
    let plugin = Plugin::named("test", "0.1.0").widget_as::<Probe<0>>("battery");
    let mut peer = Peer::start(plugin, vec![Peer::battery(1, Some(0.5))]).await;
    peer.published("battery", "").await;
    peer.render("battery", "bar-1", "").await;
    peer.patch(vec![Peer::battery(2, Some(0.5))]).await;
    peer.quiet().await;
    peer.patch(vec![Peer::battery(3, None)]).await;
    assert!(peer.published("battery", "").await.root.is_none());
    assert!(peer.published("battery", "bar-1").await.root.is_none());
    peer.patch(vec![Peer::battery(2, Some(0.5)), Peer::battery(4, None)])
        .await;
    peer.quiet().await;
    peer.patch(vec![Peer::battery(5, Some(0.5))]).await;
    assert_eq!(Peer::text(&peer.published("battery", "").await), "0.5");
    assert_eq!(Peer::text(&peer.published("battery", "bar-1").await), "0.5");
}

#[tokio::test]
async fn reconfiguration_and_removal_replace_publication_history() {
    let plugin = Plugin::named("test", "0.1.0").widget_as::<Probe<0>>("battery");
    let mut peer = Peer::start(plugin, vec![Peer::battery(1, Some(0.5))]).await;
    peer.published("battery", "").await;
    peer.render("battery", "bar-1", "old:").await;
    assert_eq!(
        Peer::text(&peer.render("battery", "bar-1", "new:").await),
        "new:0.5"
    );
    peer.patch(vec![Peer::battery(2, Some(0.7))]).await;
    assert_eq!(Peer::text(&peer.published("battery", "").await), "0.7");
    assert_eq!(
        Peer::text(&peer.published("battery", "bar-1").await),
        "new:0.7"
    );
    assert!(matches!(
        peer.invoke(invoke::Op::RemoveWidget(RemoveWidget {
            surface_id: "battery".into(),
            module_id: "bar-1".into(),
        }))
        .await,
        result::Outcome::Ok(_)
    ));
    peer.patch(vec![Peer::battery(3, Some(0.5))]).await;
    peer.published("battery", "").await;
    peer.quiet().await;
    assert_eq!(
        Peer::text(&peer.render("battery", "bar-1", "fresh:").await),
        "fresh:0.5"
    );
    peer.patch(vec![Peer::battery(4, Some(0.7))]).await;
    peer.published("battery", "").await;
    assert_eq!(
        Peer::text(&peer.published("battery", "bar-1").await),
        "fresh:0.7"
    );
}

#[tokio::test]
async fn explicit_absence_is_ready_and_empty_first_views_are_published() {
    let plugin = Plugin::named("test", "0.1.0").widget_as::<Probe<0>>("battery");
    let mut peer = Peer::start(plugin, vec![Peer::battery(1, None)]).await;
    assert!(peer.published("battery", "").await.root.is_none());
    peer.patch(vec![Peer::battery(2, None)]).await;
    peer.quiet().await;
}

struct Forward {
    context: Context,
}
impl Wired for Forward {
    fn topics() -> Vec<SystemTopic> {
        vec![]
    }
    fn capabilities() -> Vec<Capability> {
        vec![Capability::Notify]
    }
    fn build(context: &Context, _: &Values) -> Self {
        Self {
            context: context.clone(),
        }
    }
}
impl crate::Command for Forward {
    type Output = ();
    async fn call(&self, _: Args) -> Result<(), crate::Error> {
        self.context
            .act(invoke::Op::Act(omega_proto::omega::Act {
                action: Some(omega_proto::omega::Action {
                    kind: Some(omega_proto::omega::action::Kind::Notify(Default::default())),
                }),
            }))?
            .wait()
            .await?;
        Ok(())
    }
}
impl Peer {
    async fn command_start(&mut self) -> (u64, u64) {
        let command = self.streams.allocate();
        self.send(Frame {
            stream_id: command,
            body: Some(frame::Body::Invoke(Invoke {
                op: Some(invoke::Op::CallCommand(omega_proto::omega::CallCommand {
                    command: "forward".into(),
                    args: vec![],
                })),
            })),
        })
        .await;
        let effect = self.next().await;
        assert!(matches!(
            effect.body,
            Some(frame::Body::Invoke(Invoke {
                op: Some(invoke::Op::Act(_))
            }))
        ));
        (command, effect.stream_id)
    }
}

#[tokio::test]
async fn effect_completions_are_correlated_without_blocking_state_or_other_requests() {
    let plugin = Plugin::named("test", "0.1.0")
        .command::<Forward>("forward")
        .widget_as::<Probe<0>>("battery");
    let mut peer = Peer::start(plugin, vec![Peer::battery(1, Some(0.5))]).await;
    peer.published("battery", "").await;
    let (first, effect1) = peer.command_start().await;
    let (second, effect2) = peer.command_start().await;
    peer.patch(vec![Peer::battery(2, Some(0.7))]).await;
    peer.published("battery", "").await;
    peer.quiet().await;
    peer.send(omega_proto::Refusal::denied("no notification grant").frame(effect2))
        .await;
    let answer = peer.next().await;
    assert_eq!(answer.stream_id, second);
    let refusal = omega_proto::Refusal::of(&answer).unwrap();
    assert_eq!(
        refusal.code,
        omega_proto::omega::ErrorCode::PermissionDenied
    );
    assert_eq!(refusal.message, "no notification grant");
    peer.send(Frame {
        stream_id: effect1,
        body: Some(frame::Body::Result(OpResult {
            done: true,
            outcome: Some(result::Outcome::Ok(Default::default())),
        })),
    })
    .await;
    let answer = peer.next().await;
    assert_eq!(answer.stream_id, first);
    assert!(matches!(
        answer.body,
        Some(frame::Body::Result(OpResult {
            outcome: Some(result::Outcome::Ok(_)),
            ..
        }))
    ));
    peer.quiet().await;
}

#[tokio::test(start_paused = true)]
async fn a_forwarded_timeout_is_answered_and_a_late_refusal_does_not_kill_the_runtime() {
    let mut peer = Peer::start(
        Plugin::named("test", "0.1.0").command::<Forward>("forward"),
        vec![],
    )
    .await;
    let (command, effect) = peer.command_start().await;
    tokio::time::advance(crate::effect::queue::Effects::TIMEOUT).await;
    let answer = peer.next().await;
    assert_eq!(answer.stream_id, command);
    assert!(
        omega_proto::Refusal::of(&answer)
            .unwrap()
            .message
            .contains("deadline")
    );
    peer.send(omega_proto::Refusal::denied("late").frame(effect))
        .await;
    peer.quiet().await;
}

#[tokio::test]
async fn completion_saturation_refuses_new_commands_before_they_submit_effects() {
    let mut peer = Peer::start(
        Plugin::named("test", "0.1.0").command::<Forward>("forward"),
        vec![],
    )
    .await;
    for _ in 0..Runtime::COMMAND_LIMIT {
        peer.command_start().await;
    }
    let answer = peer
        .invoke(invoke::Op::CallCommand(omega_proto::omega::CallCommand {
            command: "forward".into(),
            args: vec![],
        }))
        .await;
    let result::Outcome::Error(error) = answer else {
        panic!("expected overload refusal")
    };
    assert!(error.message.contains("capacity"));
    peer.quiet().await;
}

#[tokio::test]
async fn oversized_pull_results_are_refused_without_installing_or_caching_the_instance() {
    let plugin = Plugin::named("test", "0.1.0").widget_as::<Probe<0>>("battery");
    let mut peer = Peer::start(plugin, vec![Peer::battery(1, Some(0.5))]).await;
    peer.published("battery", "").await;
    let answer = peer
        .invoke(invoke::Op::RenderWidget(RenderWidget {
            surface_id: "battery".into(),
            module_id: "too-large".into(),
            config: Values::new().with("label", "oversized").into_map(),
        }))
        .await;
    let result::Outcome::Error(error) = answer else {
        panic!("expected refusal")
    };
    assert_eq!(
        error.code,
        omega_proto::omega::ErrorCode::PayloadTooLarge as i32
    );
    peer.patch(vec![Peer::battery(2, Some(0.7))]).await;
    peer.published("battery", "").await;
    peer.quiet().await;
}

#[derive(crate::Command)]
struct Sequence {
    notify: crate::effect::Notify,
}
impl crate::Command for Sequence {
    type Output = String;
    async fn call(&self, _: Args) -> Result<String, crate::Error> {
        self.notify.send("first").await?;
        self.notify.send("second").await?;
        Ok("finished".into())
    }
}

#[tokio::test]
async fn an_async_command_sequences_effects_and_returns_a_typed_value() {
    let mut peer = Peer::start(
        Plugin::named("test", "0.1.0").command::<Sequence>("forward"),
        vec![],
    )
    .await;
    let (command, first) = peer.command_start().await;
    peer.quiet().await;
    peer.send(Frame::reply(first, result::Outcome::Ok(Default::default())))
        .await;
    let second = peer.next().await;
    let Some(frame::Body::Invoke(Invoke {
        op: Some(invoke::Op::Act(act)),
    })) = second.body
    else {
        panic!("second effect was not submitted");
    };
    assert!(matches!(act.action.unwrap().kind,
        Some(omega_proto::omega::action::Kind::Notify(notify)) if notify.summary == "second"));
    peer.quiet().await;
    peer.send(Frame::reply(
        second.stream_id,
        result::Outcome::Ok(Default::default()),
    ))
    .await;
    let answer = peer.next().await;
    assert_eq!(answer.stream_id, command);
    assert!(matches!(answer.body, Some(frame::Body::Result(OpResult {
        outcome: Some(result::Outcome::Value(value)), done: true,
    })) if omega_proto::FromValue::from_value(&value) == Some("finished".to_string())));
}

#[tokio::test]
async fn question_mark_preserves_the_refusal_and_skips_later_effects() {
    let mut peer = Peer::start(
        Plugin::named("test", "0.1.0").command::<Sequence>("forward"),
        vec![],
    )
    .await;
    let (command, first) = peer.command_start().await;
    peer.send(omega_proto::Refusal::denied("not allowed").frame(first))
        .await;
    let answer = peer.next().await;
    assert_eq!(answer.stream_id, command);
    assert_eq!(
        omega_proto::Refusal::of(&answer).unwrap().code,
        omega_proto::omega::ErrorCode::PermissionDenied
    );
    peer.quiet().await;
}

#[derive(crate::Command)]
struct Delayed {
    notify: crate::effect::Notify,
}
impl crate::Command for Delayed {
    type Output = ();
    async fn call(&self, _: Args) -> Result<(), crate::Error> {
        self.notify.send("started").await?;
        tokio::time::sleep(Duration::from_secs(60)).await;
        self.notify.send("finished").await
    }
}

#[tokio::test(start_paused = true)]
async fn disconnect_releases_a_runtime_with_an_unfinished_command() {
    let mut peer = Peer::start(
        Plugin::named("test", "0.1.0").command::<Delayed>("forward"),
        vec![],
    )
    .await;
    let (_, first) = peer.command_start().await;
    peer.send(Frame::reply(first, result::Outcome::Ok(Default::default())))
        .await;
    peer.quiet().await;
    let (replacement, _other) = UnixStream::pair().unwrap();
    drop(std::mem::replace(
        &mut peer.transport,
        Transport::new(replacement),
    ));
    tokio::time::timeout(Duration::from_secs(1), &mut peer.task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
}
