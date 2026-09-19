//! Protocol-level plugin test harness.

use std::collections::HashMap;

use omega_proto::Manifest;
use omega_proto::omega::{
    CallCommand, Frame, Hello, Invoke, PublishView, RenderWidget, Value, Welcome, frame, invoke,
    result,
};
use omega_proto::{PROTOCOL_VERSION, Transport, Values};
use tokio::net::UnixStream;

use crate::plugin::Plugin;
use crate::testing::drawn::Drawn;
use crate::testing::state::State;

/// Run a plugin against an in-process daemon over a socket pair.
/// The harness grants declared capabilities and publishes supplied fixtures.
/// Protocol violations panic and fail the test.
#[derive(Debug)]
pub struct TestDaemon {
    transport: Transport<UnixStream>,
    streams: omega_proto::DaemonStreams,
    revision: u64,
}

impl TestDaemon {
    /// Run a plugin against a test daemon. The plugin serves on a task of its
    /// own until the returned daemon is dropped.
    pub fn serving(plugin: Plugin) -> Self {
        let (daemon, plugin_stream) =
            UnixStream::pair().expect("a socket pair is always available");
        let program = plugin
            .program
            .prepare()
            .expect("the plugin's manifest is valid");

        tokio::spawn(async move {
            if let Ok(runtime) = crate::runtime::Runtime::over(plugin_stream, program).await {
                let _ = runtime.serve().await;
            }
        });

        Self {
            transport: Transport::new(daemon),
            streams: omega_proto::DaemonStreams::new(),
            revision: 0,
        }
    }

    /// Complete the handshake, handing the plugin its opening state. Returns
    /// what the plugin said about itself.
    pub async fn welcome(&mut self, state: &State) -> Hello {
        self.welcome_configured(state, &Values::new()).await
    }

    /// Complete the handshake with explicit plugin settings.
    /// Surface placement settings can be supplied separately during rendering.
    pub async fn welcome_configured(&mut self, state: &State, settings: &Values) -> Hello {
        let hello = match self.next().await.body {
            Some(frame::Body::Hello(hello)) => hello,
            other => panic!("the plugin's first frame was {other:?}, not a Hello"),
        };

        let mut snapshot = state.snapshot();
        for topic in &mut snapshot.topics {
            self.revision += 1;
            topic.revision = self.revision;
        }
        self.send(Frame {
            stream_id: 0,
            body: Some(frame::Body::Welcome(Welcome {
                host_assignment: None,
                protocol_version: PROTOCOL_VERSION,
                plugin_id: "test".to_string(),
                daemon_version: env!("CARGO_PKG_VERSION").to_string(),
                capabilities: Vec::new(),
                state: Some(snapshot),
                config: settings.clone().into_map(),
            })),
        })
        .await;

        hello
    }

    /// Publish state and trigger dependent surface updates.
    pub async fn publish(&mut self, state: &State) {
        let mut patch = state.patch();
        for topic in &mut patch.topics {
            self.revision += 1;
            topic.revision = self.revision;
        }
        self.send(Frame {
            stream_id: 0,
            body: Some(frame::Body::StatePatch(patch)),
        })
        .await;
    }

    /// Request a surface instance and return its initial view.
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
                instance: Some(omega_proto::omega::InstanceRef {
                    id: format!("test-{surface}-{module}"),
                    incarnation: "test-session".into(),
                }),
                config,
            }),
        )
        .await;

        match self.answer(stream).await {
            result::Outcome::View(view) => Drawn { tree: view },
            other => panic!("{surface} answered RenderWidget with {other:?}, not a view"),
        }
    }

    /// Invoke a command and return its outcome, including any refusal.
    pub async fn call(
        &mut self,
        command: &str,
        args: Vec<Value>,
    ) -> std::result::Result<Option<Value>, String> {
        let stream = self.streams.allocate();
        self.invoke(
            stream,
            invoke::Op::CallCommand(CallCommand {
                invocation_id: 0,
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
                        instance,
                        view: Some(view),
                    })),
            })) = self.next().await.body
            {
                return Published {
                    surface: surface_id,
                    instance: omega_proto::instance::InstanceKey::try_from(
                        &instance.expect("published instance"),
                    )
                    .expect("valid instance"),
                    view: Drawn { tree: view },
                };
            }
        }
    }

    /// The next thing the plugin asked the machine to do.
    pub async fn next_effect(&mut self) -> invoke::Op {
        loop {
            if let Some(frame::Body::Invoke(Invoke { op: Some(op) })) = self.next().await.body
                && !matches!(op, invoke::Op::PublishView(_))
            {
                return op;
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
        let frame = self
            .transport
            .recv()
            .await
            .expect("the connection to the plugin broke")
            .expect("the plugin closed the connection");
        if matches!(frame.body, Some(frame::Body::Invoke(_))) {
            self.send(Frame {
                stream_id: frame.stream_id,
                body: Some(frame::Body::Result(omega_proto::omega::Result {
                    done: true,
                    outcome: Some(result::Outcome::Ok(Default::default())),
                })),
            })
            .await;
        }
        frame
    }
}

/// A view a plugin published, and which instance of which surface it was for.
#[derive(Debug, Clone)]
pub struct Published {
    pub surface: String,
    pub instance: omega_proto::instance::InstanceKey,
    pub view: Drawn,
}

/// The manifest a plugin declares, for asserting on what it asked for.
pub fn manifest_of(plugin: &Plugin) -> Manifest {
    plugin.manifest().expect("the plugin's manifest is valid")
}
