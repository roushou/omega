//! A daemon that is really a test.

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
    streams: omega_proto::DaemonStreams,
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
            streams: omega_proto::DaemonStreams::new(),
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
