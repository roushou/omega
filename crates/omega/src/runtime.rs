//! The loop nobody writing a plugin should have to see.
//!
//! Connect, prove who we are, mirror the state the daemon replicates, build
//! one value per instance, render when something moves, answer what is asked,
//! and post whatever effects were queued. A plugin author writes none of it.

use std::collections::HashMap;

use omega_proto::Manifest;
use omega_proto::omega::{Frame, Invoke, PublishView, Result as OpResult, frame, invoke, result};
use omega_proto::{Client, Handshake, Socket, Values};
use tokio::net::UnixStream;

use crate::context::Context;
use crate::error::Error;
use crate::plugin::Plugin;
use crate::registry::{CalledCommand, RenderedWidget};
use crate::surface::{Answer, Args};

/// One instance of one surface: what the document asked for, and the value
/// built to serve it.
struct Instance {
    surface: String,
    module: String,
    widget: Box<dyn RenderedWidget>,
}

pub(crate) struct Runtime {
    client: Client,
    context: Context,
    effects: tokio::sync::mpsc::UnboundedReceiver<invoke::Op>,
    /// What the document configured this unit with, as the daemon handed it
    /// over at the handshake. Every surface is built out of it, and a widget
    /// instance's own settings are laid over it.
    settings: Values,
}

impl Runtime {
    /// Connect over the conventional socket, with the token this process was
    /// spawned with.
    pub(crate) async fn connect(manifest: &Manifest) -> Result<Self, Error> {
        Self::over_socket(&Socket::resolve(), manifest).await
    }

    pub(crate) async fn over_socket(socket: &Socket, manifest: &Manifest) -> Result<Self, Error> {
        let (client, welcome) =
            Client::connect(socket, &manifest.hash(), &Handshake::token_from_env()).await?;
        Ok(Self::welcomed(client, welcome))
    }

    /// Connect over a stream that is already open — a `UnixStream::pair` in a
    /// test, where there is no daemon and no socket file.
    pub(crate) async fn over(stream: UnixStream, manifest: &Manifest) -> Result<Self, Error> {
        let (client, welcome) =
            Client::over(stream, &manifest.hash(), &Handshake::token_from_env()).await?;
        Ok(Self::welcomed(client, welcome))
    }

    fn welcomed(client: Client, welcome: omega_proto::omega::Welcome) -> Self {
        let (sender, effects) = tokio::sync::mpsc::unbounded_channel();
        Self {
            client,
            context: Context::new(&welcome.state.unwrap_or_default(), sender),
            effects,
            settings: Values::from_map(welcome.config),
        }
    }

    /// Serve every surface the plugin registered until the daemon closes.
    pub(crate) async fn serve(mut self, plugin: Plugin) -> Result<(), Error> {
        let wanted = plugin.topics();

        // Until the document says otherwise, each widget surface has one
        // instance. Building it now means the first render is of the snapshot
        // the Welcome carried rather than of nothing.
        let mut instances: Vec<Instance> = plugin
            .widgets()
            .iter()
            .map(|entry| Instance {
                surface: entry.surface.clone(),
                module: String::new(),
                widget: entry.build(&self.context, &self.settings),
            })
            .collect();

        // A command is built once: it holds handles, not state, and there is
        // one of it however many times it is called.
        let commands: HashMap<String, Box<dyn CalledCommand>> = plugin
            .commands()
            .iter()
            .map(|entry| {
                (
                    entry.name.clone(),
                    entry.build(&self.context, &self.settings),
                )
            })
            .collect();

        let reactions: Vec<_> = plugin
            .reactions()
            .iter()
            .map(|entry| (entry.event, entry.build(&self.context, &self.settings)))
            .collect();

        // A widget that declared a topic is not asked to draw a machine it
        // cannot see yet.
        let mut ready = self.context.holds(&wanted);
        if ready {
            self.publish_all(&instances).await?;
        }

        loop {
            tokio::select! {
                // Something a field queued on its way out.
                Some(op) = self.effects.recv() => {
                    self.client.invoke(0, op).await?;
                }
                received = self.client.recv() => {
                    let Some(frame) = received? else { return Ok(()) };

                    match frame.body {
                        Some(frame::Body::StatePatch(patch)) => {
                            self.context.apply(&patch);
                            if !ready {
                                ready = self.context.holds(&wanted);
                            }
                            if ready {
                                self.publish_all(&instances).await?;
                            }
                        }

                        // The daemon handing a widget an instance of itself,
                        // with the settings the document gave it.
                        Some(frame::Body::Invoke(Invoke {
                            op: Some(invoke::Op::RenderWidget(render)),
                        })) => {
                            let Some(entry) = plugin
                                .widgets()
                                .iter()
                                .find(|entry| entry.surface == render.surface_id)
                            else {
                                continue;
                            };

                            // What this placement said, over what the unit
                            // was configured with: naming one key where a
                            // widget is placed must not reset the rest.
                            let settings =
                                Values::from_map(render.config.clone()).over(&self.settings);
                            let widget = entry.build(&self.context, &settings);
                            let view = widget.render();

                            self.answer(frame.stream_id, result::Outcome::View(view.clone().into_tree())).await?;

                            instances.retain(|held| {
                                held.surface != render.surface_id
                                    || (!held.module.is_empty() && held.module != render.module_id)
                            });
                            instances.push(Instance {
                                surface: render.surface_id,
                                module: render.module_id,
                                widget,
                            });
                        }

                        Some(frame::Body::Invoke(Invoke {
                            op: Some(invoke::Op::CallCommand(call)),
                        })) => {
                            let answer = match commands.get(&call.command) {
                                Some(command) => command.call(Args::new(call.args)),
                                None => Answer::refused(format!("no command {}", call.command)),
                            };
                            self.answer(frame.stream_id, Self::outcome(answer)).await?;
                        }

                        Some(frame::Body::Event(event)) => {
                            for (kind, reaction) in &reactions {
                                if event.kind == *kind as i32 {
                                    reaction.fire(&event);
                                }
                            }
                        }

                        _ => {}
                    }
                }
            }
        }
    }

    async fn publish_all(&mut self, instances: &[Instance]) -> Result<(), Error> {
        for instance in instances {
            let view = instance.widget.render();
            self.client
                .invoke(
                    0,
                    invoke::Op::PublishView(PublishView {
                        surface_id: instance.surface.clone(),
                        module_id: instance.module.clone(),
                        view: Some(view.into_tree()),
                    }),
                )
                .await?;
        }
        Ok(())
    }

    async fn answer(&mut self, stream_id: u64, outcome: result::Outcome) -> Result<(), Error> {
        self.client
            .send(Frame {
                stream_id,
                body: Some(frame::Body::Result(OpResult {
                    outcome: Some(outcome),
                    done: true,
                })),
            })
            .await?;
        Ok(())
    }

    fn outcome(answer: Answer) -> result::Outcome {
        match answer {
            Answer::Done => result::Outcome::Ok(Default::default()),
            Answer::Value(value) => result::Outcome::Value(value),
            Answer::Refused(message) => result::Outcome::Error(omega_proto::omega::Error {
                code: omega_proto::omega::ErrorCode::InvalidArgument as i32,
                message,
            }),
        }
    }
}
