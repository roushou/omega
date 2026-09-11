//! The plugin runtime loop.
//!
//! Connect, prove who we are, mirror the state the daemon replicates, build
//! one value per instance, render when something moves, answer what is asked,
//! and post whatever effects were queued. A plugin author writes none of it.

use std::collections::HashMap;

use omega_proto::Manifest;
use omega_proto::omega::{Frame, Invoke, PublishView, frame, invoke, result};
use omega_proto::{Client, Handshake, Socket, Values};
use tokio::net::UnixStream;

use crate::context::Context;
use crate::error::Error;
use crate::plugin::Plugin;
use crate::registry::CalledCommand;
use crate::surface::Args;

mod instance;
#[cfg(test)]
mod tests;

use instance::Instance;

pub(crate) struct Runtime {
    client: Client,
    context: Context,
    effects: crate::effect::queue::Effects,
    /// What the document configured this unit with, as the daemon handed it
    /// over at the handshake. Every surface is built out of it, and a widget
    /// instance's own settings are laid over it.
    settings: Values,
}

impl Runtime {
    const COMMAND_LIMIT: usize = 64;
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
        let (sender, effects) = crate::effect::queue::Effects::channel();
        Self {
            client,
            context: Context::new(&welcome.state.unwrap_or_default(), sender),
            effects,
            settings: Values::from_map(welcome.config),
        }
    }

    /// Serve every surface the plugin registered until the daemon closes.
    pub(crate) async fn serve(mut self, plugin: Plugin) -> Result<(), Error> {
        // Until the document says otherwise, each widget surface has one
        // instance. Building it now means the first render is of the snapshot
        // the Welcome carried rather than of nothing.
        let mut instances: Vec<Instance> = plugin
            .widgets()
            .iter()
            .map(|entry| Instance::new(entry, String::new(), &self.context, &self.settings))
            .collect();

        // A command is built once: it holds handles, not state, and there is
        // one of it however many times it is called.
        let commands: HashMap<String, std::sync::Arc<dyn CalledCommand>> = plugin
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

        self.publish_all(&mut instances).await?;

        let mut answers = tokio::task::JoinSet::new();
        loop {
            let deadline = self.effects.deadline();
            tokio::select! {
                _ = async { tokio::time::sleep_until(deadline.expect("enabled deadline")).await }, if deadline.is_some() => {
                    self.effects.expire()?;
                }
                Some(answer) = answers.join_next(), if !answers.is_empty() => {
                    let (stream, completion) = answer.map_err(|error| Error::Runtime(std::io::Error::other(error)))?;
                    self.answer(stream, Self::completion(completion)).await?;
                }
                // Something a field queued on its way out.
                Some(request) = self.effects.recv() => {
                    let stream = self.client.allocate();
                    if let Some(op) = self.effects.begin(stream, request)? {
                        self.client.invoke(stream, op).await?;
                    }
                }
                received = self.client.recv() => {
                    let Some(frame) = received? else { return Ok(()) };

                    match frame.body {
                        Some(frame::Body::StatePatch(patch)) => {
                            self.context.apply(&patch);
                            self.publish_all(&mut instances).await?;
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
                                self.client.send(omega_proto::Refusal::invalid("unknown widget surface").frame(frame.stream_id)).await?;
                                continue;
                            };

                            // What this placement said, over what the unit
                            // was configured with: naming one key where a
                            // widget is placed must not reset the rest.
                            let settings =
                                Values::from_map(render.config.clone()).over(&self.settings);
                            let mut instance = Instance::new(entry, render.module_id.clone(), &self.context, &settings);
                            let view = instance.view(&self.context).unwrap_or_default();
                            let answer = Frame::reply(frame.stream_id, result::Outcome::View(view.clone()));
                            let rendered = matches!(&answer.body, Some(frame::Body::Result(answer)) if matches!(answer.outcome, Some(result::Outcome::View(_))));
                            self.client.send(answer).await?;
                            if !rendered { continue; }
                            instance.sent(view);

                            instances.retain(|held| {
                                held.surface != render.surface_id
                                    || held.module != render.module_id
                            });
                            instances.push(instance);
                        }

                        Some(frame::Body::Invoke(Invoke {
                            op: Some(invoke::Op::CallCommand(call)),
                        })) => {
                            if answers.len() >= Self::COMMAND_LIMIT {
                                self.client.send(omega_proto::Refusal::exhausted("command completion capacity exhausted").frame(frame.stream_id)).await?;
                                continue;
                            }
                            let Some(command) = commands.get(&call.command).cloned() else {
                                self.client.send(omega_proto::Refusal::invalid(format!("no command {}", call.command)).frame(frame.stream_id)).await?;
                                continue;
                            };
                            answers.spawn(async move {
                                let completion = command.call(Args::new(call.args)).await;
                                (frame.stream_id, completion)
                            });
                        }

                        Some(frame::Body::Invoke(Invoke { op: Some(invoke::Op::RemoveWidget(remove)) })) => {
                            if remove.module_id.is_empty() {
                                self.client.send(omega_proto::Refusal::invalid("cannot remove the anonymous instance").frame(frame.stream_id)).await?;
                                continue;
                            }
                            instances.retain(|held| held.surface != remove.surface_id || held.module != remove.module_id);
                            self.answer(frame.stream_id, result::Outcome::Ok(Default::default())).await?;
                        }
                        Some(frame::Body::Invoke(_)) => {
                            self.client.send(omega_proto::Refusal::unimplemented("unsupported unit operation").frame(frame.stream_id)).await?;
                        }
                        Some(frame::Body::Event(event)) => {
                            for (kind, reaction) in &reactions {
                                if event.kind == *kind as i32 {
                                    reaction.fire(&event);
                                }
                            }
                        }

                        Some(frame::Body::Result(answer)) => {
                            if self.effects.answer(frame.stream_id, &answer)? {
                                continue;
                            }
                            if let Some(result::Outcome::Error(error)) = answer.outcome {
                                return Err(omega_proto::Refusal::new(omega_proto::omega::ErrorCode::try_from(error.code).unwrap_or_default(), error.message).into());
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    async fn publish_all(&mut self, instances: &mut [Instance]) -> Result<(), Error> {
        for instance in instances {
            let Some(view) = instance.changed(&self.context) else {
                continue;
            };
            let stream = self.client.allocate();
            self.client
                .invoke(
                    stream,
                    invoke::Op::PublishView(PublishView {
                        surface_id: instance.surface.clone(),
                        module_id: instance.module.clone(),
                        view: Some(view.clone()),
                    }),
                )
                .await?;
            instance.sent(view);
        }
        Ok(())
    }

    async fn answer(&mut self, stream_id: u64, outcome: result::Outcome) -> Result<(), Error> {
        self.client.send(Frame::reply(stream_id, outcome)).await?;
        Ok(())
    }

    fn completion(completion: Result<omega_proto::omega::Value, crate::Error>) -> result::Outcome {
        match completion {
            Ok(value) if value.kind.is_none() => result::Outcome::Ok(Default::default()),
            Ok(value) => result::Outcome::Value(value),
            Err(error) => {
                let refusal = error.refusal();
                result::Outcome::Error(omega_proto::omega::Error {
                    code: refusal.code as i32,
                    message: refusal.message,
                })
            }
        }
    }
}
