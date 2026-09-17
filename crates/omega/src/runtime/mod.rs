//! Plugin session loop: authentication, state replication, rendering,
//! command dispatch, and effect forwarding.

use std::collections::HashMap;

use omega_proto::Manifest;
use omega_proto::omega::{Frame, Invoke, PublishView, frame, invoke, result};
use omega_proto::{Client, Handshake, Socket, Values};
use tokio::net::UnixStream;

use crate::command::Args;
use crate::error::Error;
use crate::plugin::Plugin;
use crate::plugin::registry::CalledCommand;
use crate::runtime::context::Context;

mod instance;
#[cfg(test)]
mod tests;

use instance::Instance;

pub(crate) struct Runtime {
    publications: std::collections::BTreeSet<u64>,
    client: Client,
    context: Context,
    effects: crate::effect::queue::Effects,
    /// Unit settings received at handshake. Instance settings override matching keys.
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
            publications: Default::default(),
            client,
            context: Context::new(&welcome.state.unwrap_or_default(), sender),
            effects,
            settings: Values::from_map(welcome.config),
        }
    }

    /// Serve every surface the plugin registered until the daemon closes.
    pub(crate) async fn serve(mut self, plugin: Plugin) -> Result<(), Error> {
        let mut instances: Vec<Instance> = Vec::new();

        // Command instances are shared across concurrent invocations.
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
                completed = std::future::poll_fn(|cx| {
                    for instance in &mut instances {
                        if let std::task::Poll::Ready(answer) = instance.poll(cx) { return std::task::Poll::Ready(answer); }
                    }
                    std::task::Poll::Pending
                }) => {
                    completed?;
                    self.publish_all(&mut instances).await?;
                }

                _ = async { tokio::time::sleep_until(deadline.expect("enabled deadline")).await }, if deadline.is_some() => {
                    self.effects.expire()?;
                }
                Some(answer) = answers.join_next(), if !answers.is_empty() => {
                    let (stream, completion) = answer.map_err(|error| Error::Runtime(std::io::Error::other(error)))?;
                    self.answer(stream, Self::completion(completion)).await?;
                }
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
                            for instance in &mut instances { instance.invalidate(&patch); }
                            self.publish_all(&mut instances).await?;
                        }

                        Some(frame::Body::Invoke(Invoke {
                            op: Some(invoke::Op::RenderWidget(render)),
                        })) => {
                            let Some(entry) = plugin
                                .surfaces()
                                .iter()
                                .find(|entry| entry.surface == render.surface_id)
                            else {
                                self.client.send(omega_proto::Refusal::invalid("unknown widget surface").frame(frame.stream_id)).await?;
                                continue;
                            };

                            // Placement settings override only the keys they supply.
                            let settings =
                                Values::from_map(render.config.clone()).over(&self.settings);
                            let identity = match render.instance.as_ref().map(omega_proto::instance::InstanceKey::parse).transpose() {
                                Ok(Some(identity)) => identity,
                                _ => {
                                    self.client.send(omega_proto::Refusal::invalid("RenderWidget requires a valid instance identity").frame(frame.stream_id)).await?;
                                    continue;
                                }
                            };
                            if instances.len() >= 256 && !instances.iter().any(|held| held.identity == identity) {
                                self.client.send(omega_proto::Refusal::exhausted("instance capacity exhausted").frame(frame.stream_id)).await?;
                                continue;
                            }
                            let mut instance = match Instance::new(entry, identity.clone(), &self.context, &settings) {
                                Ok(instance) => instance,
                                Err(error) => { self.client.send(error.refusal().frame(frame.stream_id)).await?; continue; }
                            };
                            let view = match instance.view(&self.context) {
                                Ok(view) => view,
                                Err(error) => { self.client.send(error.refusal().frame(frame.stream_id)).await?; continue; }
                            };
                            let answer = Frame::reply(frame.stream_id, result::Outcome::View(view.clone()));
                            let rendered = matches!(&answer.body, Some(frame::Body::Result(answer)) if matches!(answer.outcome, Some(result::Outcome::View(_))));
                            self.client.send(answer).await?;
                            if !rendered { continue; }
                            instance.sent(view);

                            instances.retain(|held| {
                                held.identity != identity
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

                        Some(frame::Body::Invoke(Invoke { op: Some(invoke::Op::SurfaceLifecycle(event)) })) => {
                            let identity = event.instance.as_ref().map(omega_proto::instance::InstanceKey::parse).transpose();
                            let completion = match identity {
                                Ok(Some(identity)) => match instances.iter_mut().find(|instance| instance.identity == identity) {
                                    Some(instance) => instance.lifecycle(event.state),
                                    None => Err(omega_proto::Refusal::precondition("unknown or expired instance").into()),
                                },
                                _ => Err(crate::Error::invalid("lifecycle requires an instance")),
                            };
                            self.answer(frame.stream_id, Self::completion(completion.map(|()| Default::default()))).await?;
                            self.publish_all(&mut instances).await?;
                        }
                        Some(frame::Body::Invoke(Invoke { op: Some(invoke::Op::SurfaceEvent(event)) })) => {
                            let identity = event.instance.as_ref().map(omega_proto::instance::InstanceKey::parse).transpose();
                            let completion = match identity {
                                Ok(Some(identity)) => match instances.iter_mut().find(|instance| instance.identity == identity) {
                                    Some(instance) => instance.event(&event),
                                    None => Err(omega_proto::Refusal::precondition("unknown or expired instance").into()),
                                },
                                _ => Err(crate::Error::invalid("SurfaceEvent requires an instance")),
                            };
                            self.answer(frame.stream_id, Self::completion(completion.map(|()| Default::default()))).await?;
                            self.publish_all(&mut instances).await?;
                        }
                        Some(frame::Body::Invoke(Invoke { op: Some(invoke::Op::RemoveWidget(remove)) })) => {
                            let identity = match remove.instance.as_ref().map(omega_proto::instance::InstanceKey::parse).transpose() {
                                Ok(Some(identity)) => identity,
                                _ => {
                                    self.client.send(omega_proto::Refusal::invalid("RemoveWidget requires a valid instance identity").frame(frame.stream_id)).await?;
                                    continue;
                                }
                            };
                            instances.retain(|held| held.identity != identity);
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
                            if self.effects.answer(frame.stream_id, &answer)? { continue; }
                            if self.publications.remove(&frame.stream_id) {
                                if let Some(result::Outcome::Error(error)) = &answer.outcome
                                    && error.code != omega_proto::omega::ErrorCode::FailedPrecondition as i32 {
                                    return Err(omega_proto::Refusal::new(omega_proto::omega::ErrorCode::try_from(error.code).unwrap_or_default(), error.message.clone()).into());
                                }
                                self.publish_all(&mut instances).await?;
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
            if self.publications.len() >= 256 {
                break;
            }
            let Some(view) = instance.changed(&self.context)? else {
                continue;
            };
            let stream = self.client.allocate();
            self.client
                .invoke(
                    stream,
                    invoke::Op::PublishView(PublishView {
                        surface_id: instance.surface.clone(),
                        instance: Some(instance.identity.wire()),
                        view: Some(view.clone()),
                    }),
                )
                .await?;
            self.publications.insert(stream);
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

pub(crate) mod context;
pub(crate) mod mirror;
