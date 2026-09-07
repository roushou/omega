//! Testing a plugin without a daemon.
//!
//! A widget is a function from state to a tree, so most of what can be wrong
//! with one is wrong before any socket exists. Build the plugin against the
//! state you want and look at what it drew:
//!
//! ```
//! use omega::testing::{Drawn, State};
//! use omega::{Battery, Text, Ui, Widget};
//!
//! #[derive(omega::Widget)]
//! struct Charge {
//!     battery: Battery,
//! }
//!
//! impl Widget for Charge {
//!     fn render(&self) -> Ui {
//!         Text::new(self.battery.charge()).into()
//!     }
//! }
//!
//! let drawn = Drawn::of::<Charge>(&State::new().battery(0.8, false));
//! assert_eq!(drawn.text(), "80%");
//! ```
//!
//! The rest — what a command answers, which instances a document hands a
//! widget, whether a plugin publishes at all — needs the protocol, so
//! [`TestDaemon`] speaks it over a `UnixStream::pair`. There is no listener,
//! no socket file, and no daemon: the test *is* the daemon.

use std::collections::HashMap;

use omega_manifest::Manifest;
use omega_wire::omega::{
    BatteryState, CallCommand, Frame, Hello, Invoke, NetworkState, PublishView, RenderWidget,
    StatePatch, StateSnapshot, StateTopic, Value, ViewNode, Welcome, frame, invoke, result, value,
};
use omega_wire::{PROTOCOL_VERSION, TopicValue, Transport, Values};
use tokio::net::UnixStream;

use crate::context::Context;
use crate::plugin::Plugin;
use crate::surface::{Answer, Args, Command, Widget};
use crate::ui::Ui;

/// The state a plugin reads, built for a test.
///
/// Set topics with the same types the daemon publishes, so a test cannot
/// describe a machine the daemon could not.
#[derive(Debug, Default, Clone)]
pub struct State {
    topics: Vec<StateTopic>,
}

impl State {
    pub fn new() -> Self {
        Self::default()
    }

    /// Set a topic: `State::new().with(BatteryState { .. })`.
    pub fn with<T: TopicValue>(mut self, value: T) -> Self {
        let topic = T::TOPIC.as_str().to_string();
        self.topics.retain(|held| held.topic != topic);
        self.topics.push(StateTopic {
            topic,
            revision: self.topics.len() as u64 + 1,
            value: Some(value.into_value()),
        });
        self
    }

    /// The battery, spelled the way a test means it.
    pub fn battery(self, charge: f64, charging: bool) -> Self {
        self.with(BatteryState {
            level: charge,
            charging,
            seconds_to_empty: 0,
            seconds_to_full: 0,
        })
    }

    /// A plugin's own state, as the daemon replicates it.
    pub fn keyspace(mut self, address: &str, values: Values) -> Self {
        self.topics.retain(|held| held.topic != address);
        self.topics.push(StateTopic {
            topic: address.to_string(),
            revision: self.topics.len() as u64 + 1,
            value: Some(omega_wire::omega::state_topic::Value::Generic(
                omega_wire::IntoValue::into_value(values),
            )),
        });
        self
    }

    /// A connected network of this name and strength.
    pub fn network(self, ssid: &str, signal_percent: u32) -> Self {
        self.with(NetworkState {
            connected: true,
            ssid: ssid.to_string(),
            interface: "wlan0".to_string(),
            signal_percent,
            r#type: 0,
        })
    }

    fn snapshot(&self) -> StateSnapshot {
        StateSnapshot {
            topics: self.topics.clone(),
        }
    }

    fn patch(&self) -> StatePatch {
        StatePatch {
            topics: self.topics.clone(),
        }
    }

    /// A context holding this state, with effects collected rather than sent.
    fn context(&self) -> (Context, tokio::sync::mpsc::UnboundedReceiver<invoke::Op>) {
        let (sender, effects) = tokio::sync::mpsc::unbounded_channel();
        (Context::new(&self.snapshot(), sender), effects)
    }
}

/// What a widget drew, asked questions rather than destructured.
#[derive(Debug, Clone)]
pub struct Drawn {
    tree: omega_wire::omega::ViewTree,
}

impl Drawn {
    /// Build a widget against some state and render it once.
    pub fn of<W: Widget>(state: &State) -> Self {
        Self::configured::<W>(state, &Values::new())
    }

    /// The same, for one instance the document configured.
    pub fn configured<W: Widget>(state: &State, settings: &Values) -> Self {
        let (context, _effects) = state.context();
        Self::of_ui(W::build(&context, settings).render())
    }

    pub fn of_ui(ui: Ui) -> Self {
        Self {
            tree: ui.into_tree(),
        }
    }

    /// Every text node, in tree order, separated by a space. What the widget
    /// reads as, which is usually the whole assertion.
    pub fn text(&self) -> String {
        self.nodes()
            .into_iter()
            .filter_map(Self::text_prop)
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// One node's text, by its key.
    pub fn text_of(&self, key: &str) -> Option<String> {
        Self::text_prop(self.node(key)?)
    }

    /// One node's colour, by its key.
    pub fn color_of(&self, key: &str) -> Option<String> {
        self.prop(key, "color")
    }

    /// Any string property of any node, for what the accessors do not cover.
    pub fn prop(&self, key: &str, prop: &str) -> Option<String> {
        match self.node(key)?.props.get(prop)?.kind.as_ref()? {
            value::Kind::StringValue(text) => Some(text.clone()),
            _ => None,
        }
    }

    /// The node with this key, wherever it is in the tree.
    pub fn node(&self, key: &str) -> Option<&ViewNode> {
        self.nodes().into_iter().find(|node| node.key == key)
    }

    /// Every node's kind, in tree order: what the widget actually built.
    pub fn kinds(&self) -> Vec<&str> {
        self.nodes()
            .into_iter()
            .map(|node| node.r#type.as_str())
            .collect()
    }

    /// Every key in the tree, in order.
    pub fn keys(&self) -> Vec<&str> {
        self.nodes()
            .into_iter()
            .map(|node| node.key.as_str())
            .collect()
    }

    pub fn is_empty(&self) -> bool {
        self.tree.root.is_none()
    }

    fn text_prop(node: &ViewNode) -> Option<String> {
        match node.props.get("text")?.kind.as_ref()? {
            value::Kind::StringValue(text) => Some(text.clone()),
            _ => None,
        }
    }

    fn nodes(&self) -> Vec<&ViewNode> {
        let mut found = Vec::new();
        if let Some(root) = self.tree.root.as_ref() {
            Self::walk(root, &mut found);
        }
        found
    }

    fn walk<'a>(node: &'a ViewNode, found: &mut Vec<&'a ViewNode>) {
        found.push(node);
        for child in &node.children {
            Self::walk(child, found);
        }
    }
}

/// What a command did: what it answered, and what it asked the machine to do.
#[derive(Debug)]
pub struct Called {
    pub answer: Answer,
    /// The effects it queued, in order. A command that was supposed to lock
    /// the screen and did not is a command that failed.
    pub effects: Vec<invoke::Op>,
}

impl Called {
    /// Build a command against some state, call it, and collect both halves.
    pub fn of<C: Command>(state: &State, args: Vec<Value>) -> Self {
        Self::configured::<C>(state, &Values::new(), args)
    }

    /// The same, for a unit the document configured.
    ///
    /// A command is never placed anywhere, so its unit's settings are the
    /// only settings it can have.
    pub fn configured<C: Command>(state: &State, settings: &Values, args: Vec<Value>) -> Self {
        let (context, mut effects) = state.context();
        let command = C::build(&context, settings);
        let answer = command.call(Args::new(args));

        let mut queued = Vec::new();
        while let Ok(op) = effects.try_recv() {
            queued.push(op);
        }
        Self {
            answer,
            effects: queued,
        }
    }

    /// Whether it asked for exactly this action.
    pub fn did(&self, action: &omega_wire::omega::action::Kind) -> bool {
        self.effects.iter().any(|op| match op {
            invoke::Op::Act(act) => {
                act.action.as_ref().and_then(|action| action.kind.as_ref()) == Some(action)
            }
            _ => false,
        })
    }
}

/// A daemon that is really a test.
///
/// It speaks the daemon's half of the protocol over a socket pair and does
/// nothing it is not asked to: it refuses nothing, grants everything, and
/// publishes exactly the state it is handed. The point is to let a plugin's
/// own loop run so a test can watch what comes out of it.
///
/// Protocol violations panic. A plugin that answers the wrong frame has
/// failed the test, and returning an error for the test to unwrap would only
/// move the panic.
#[derive(Debug)]
pub struct TestDaemon {
    transport: Transport<UnixStream>,
    streams: omega_wire::DaemonStreams,
}

impl TestDaemon {
    /// Run a plugin against a test daemon. The plugin serves on a task of its
    /// own until the returned daemon is dropped.
    pub fn serving(plugin: Plugin) -> Self {
        let (daemon, unit) = UnixStream::pair().expect("a socket pair is always available");
        let manifest = plugin.manifest().expect("the plugin's manifest is valid");

        tokio::spawn(async move {
            if let Ok(runtime) = crate::runtime::Runtime::over(unit, &manifest).await {
                let _ = runtime.serve(plugin).await;
            }
        });

        Self {
            transport: Transport::new(daemon),
            streams: omega_wire::DaemonStreams::new(),
        }
    }

    /// Complete the handshake, handing the plugin its opening state. Returns
    /// what the plugin said about itself.
    pub async fn welcome(&mut self, state: &State) -> Hello {
        self.welcome_configured(state, &Values::new()).await
    }

    /// The same, for a unit the document configured.
    ///
    /// These are the unit's settings, which every surface is built from — the
    /// only way a command or a reaction is configured at all, since neither
    /// is ever placed anywhere to be configured there.
    pub async fn welcome_configured(&mut self, state: &State, settings: &Values) -> Hello {
        let hello = match self.next().await.body {
            Some(frame::Body::Hello(hello)) => hello,
            other => panic!("the plugin's first frame was {other:?}, not a Hello"),
        };

        self.send(Frame {
            stream_id: 0,
            body: Some(frame::Body::Welcome(Welcome {
                protocol_version: PROTOCOL_VERSION,
                unit_id: "test".to_string(),
                daemon_version: env!("CARGO_PKG_VERSION").to_string(),
                capabilities: Vec::new(),
                state: Some(state.snapshot()),
                config: settings.clone().into_map(),
            })),
        })
        .await;

        hello
    }

    /// Move the state. A widget re-renders on its own.
    pub async fn publish(&mut self, state: &State) {
        self.send(Frame {
            stream_id: 0,
            body: Some(frame::Body::StatePatch(state.patch())),
        })
        .await;
    }

    /// Hand a widget an instance of its surface, as a document would, and
    /// take the view it answers with.
    pub async fn render(
        &mut self,
        surface: &str,
        module: &str,
        config: HashMap<String, Value>,
    ) -> Drawn {
        let stream = self.streams.allocate();
        self.invoke(
            stream,
            invoke::Op::RenderWidget(RenderWidget {
                surface_id: surface.to_string(),
                module_id: module.to_string(),
                config,
            }),
        )
        .await;

        match self.answer(stream).await {
            result::Outcome::View(view) => Drawn { tree: view },
            other => panic!("{surface} answered RenderWidget with {other:?}, not a view"),
        }
    }

    /// Call a command, and hand back what it answered — including the message
    /// it refused with, which is worth asserting on.
    pub async fn call(
        &mut self,
        command: &str,
        args: Vec<Value>,
    ) -> std::result::Result<Option<Value>, String> {
        let stream = self.streams.allocate();
        self.invoke(
            stream,
            invoke::Op::CallCommand(CallCommand {
                command: command.to_string(),
                args,
            }),
        )
        .await;

        match self.answer(stream).await {
            result::Outcome::Value(value) => Ok(Some(value)),
            result::Outcome::Ok(_) => Ok(None),
            result::Outcome::Error(error) => Err(error.message),
            other => panic!("{command} answered with {other:?}"),
        }
    }

    /// The next view the plugin publishes, skipping everything else it says.
    pub async fn next_view(&mut self) -> Published {
        loop {
            if let Some(frame::Body::Invoke(Invoke {
                op:
                    Some(invoke::Op::PublishView(PublishView {
                        surface_id,
                        module_id,
                        view: Some(view),
                    })),
            })) = self.next().await.body
            {
                return Published {
                    surface: surface_id,
                    module: module_id,
                    view: Drawn { tree: view },
                };
            }
        }
    }

    /// The next thing the plugin asked the machine to do.
    pub async fn next_effect(&mut self) -> invoke::Op {
        loop {
            if let Some(frame::Body::Invoke(Invoke { op: Some(op) })) = self.next().await.body {
                if !matches!(op, invoke::Op::PublishView(_)) {
                    return op;
                }
            }
        }
    }

    async fn invoke(&mut self, stream_id: u64, op: invoke::Op) {
        self.send(Frame {
            stream_id,
            body: Some(frame::Body::Invoke(Invoke { op: Some(op) })),
        })
        .await;
    }

    async fn answer(&mut self, stream: u64) -> result::Outcome {
        loop {
            let frame = self.next().await;
            if frame.stream_id != stream {
                continue;
            }
            if let Some(frame::Body::Result(result)) = frame.body {
                return result
                    .outcome
                    .expect("a Result the plugin sent carries no outcome");
            }
        }
    }

    async fn send(&mut self, frame: Frame) {
        self.transport
            .send(frame)
            .await
            .expect("the plugin closed the connection");
    }

    async fn next(&mut self) -> Frame {
        self.transport
            .recv()
            .await
            .expect("the connection to the plugin broke")
            .expect("the plugin closed the connection")
    }
}

/// A view a plugin published, and which instance of which surface it was for.
#[derive(Debug, Clone)]
pub struct Published {
    pub surface: String,
    pub module: String,
    pub view: Drawn,
}

/// The manifest a plugin declares, for asserting on what it asked for.
pub fn manifest_of(plugin: &Plugin) -> Manifest {
    plugin.manifest().expect("the plugin's manifest is valid")
}
