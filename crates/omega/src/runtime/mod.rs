//! Executable session loop: authentication, state replication, rendering,
//! command dispatch, and effect forwarding.

use omega_proto::omega::{Frame, Invoke, PublishView, frame, invoke, result};
use omega_proto::{Client, Handshake, Socket, Values};
use tokio::net::UnixStream;

use crate::error::Error;
use crate::program::PreparedProgram;
use crate::runtime::context::Context;
use execution::ExecutionMode;

mod commands;
mod execution;
mod instance;
#[cfg(test)]
mod tests;

use commands::{Commands, Completion};
use instance::Instance;

pub(crate) struct Runtime {
    program: PreparedProgram,
    execution: ExecutionMode,
    publications: std::collections::BTreeSet<u64>,
    client: Client,
    context: Context,
    effects: crate::effect::queue::Effects,
    /// Provider settings received at handshake. Surface instances layer their own settings.
    settings: Values,
}

impl Runtime {
    /// Connect over the conventional socket, with the token this process was
    /// spawned with.
    pub(crate) async fn connect(program: PreparedProgram) -> Result<Self, Error> {
        Self::over_socket(&Socket::resolve(), program).await
    }

    pub(crate) async fn over_socket(
        socket: &Socket,
        program: PreparedProgram,
    ) -> Result<Self, Error> {
        let (client, welcome) = Client::connect(
            socket,
            &program.manifest.hash(),
            &Handshake::token_from_env(),
        )
        .await?;
        Self::welcomed(client, welcome, program)
    }

    /// Connect over a stream that is already open — a `UnixStream::pair` in a
    /// test, where there is no daemon and no socket file.
    pub(crate) async fn over(stream: UnixStream, program: PreparedProgram) -> Result<Self, Error> {
        let (client, welcome) = Client::over(
            stream,
            &program.manifest.hash(),
            &Handshake::token_from_env(),
        )
        .await?;
        Self::welcomed(client, welcome, program)
    }

    fn welcomed(
        client: Client,
        welcome: omega_proto::omega::Welcome,
        program: PreparedProgram,
    ) -> Result<Self, Error> {
        let (sender, effects) = crate::effect::queue::Effects::channel();
        let execution = ExecutionMode::try_from((program.kind, welcome.host_assignment))?;
        Ok(Self {
            program,
            execution,
            publications: Default::default(),
            client,
            context: Context::new(&welcome.state.unwrap_or_default(), sender),
            effects,
            settings: Values::from_map(welcome.config),
        })
    }

    /// Serve the prepared registrations until disconnect or one-shot completion.
    pub(crate) async fn serve(mut self) -> Result<(), Error> {
        let mut instances: Vec<Instance> = Vec::new();

        let mut commands =
            Commands::new(&self.program.registrations, &self.context, &self.settings)?;

        let reactions: Vec<_> = self
            .program
            .registrations
            .reactions
            .iter()
            .map(|entry| (entry.event, entry.build(&self.context, &self.settings)))
            .collect();

        self.publish_all(&mut instances).await?;

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
                    self.storage_completions(&mut instances).await?;
                }
                completion = commands.next() => {
                    match completion {
                        Completion::Answer(reply) => {
                            self.client.send(*reply).await?;
                            if self.execution.complete() { return Ok(()); }
                        },
                        Completion::Failed { replies, error } => {
                            for reply in replies { self.client.send(reply).await?; }
                            return Err(error);
                        }
                    }
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
                        Some(frame::Body::StorageUpdate(update)) => {
                            if let Some(identity) = self.context.storage().apply(update) {
                                for instance in &mut instances { if instance.identity == identity { instance.storage_changed(); } }
                                self.publish_all(&mut instances).await?;
                            }
                        }
                        Some(frame::Body::StatePatch(patch)) => {
                            self.context.apply(&patch);
                            for instance in &mut instances { instance.invalidate(&patch); }
                            self.publish_all(&mut instances).await?;
                        }

                        Some(frame::Body::Invoke(Invoke {
                            op: Some(invoke::Op::RenderWidget(render)),
                        })) => {
                            let Some(entry) = self.program.registrations
                                .surfaces
                                .iter()
                                .find(|entry| entry.surface == render.surface_id)
                            else {
                                self.client.send(omega_proto::Refusal::invalid("unknown widget surface").frame(frame.stream_id)).await?;
                                continue;
                            };

                            // Placement settings override only the keys they supply.
                            let settings =
                                Values::from_map(render.config.clone()).over(&self.settings);
                            let identity = match render.instance.as_ref().map(omega_proto::instance::InstanceKey::try_from).transpose() {
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
                            let view = instance.view(&self.context);
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
                            if let Err(refusal) = self.execution.admit(call.invocation_id) {
                                self.client.send(refusal.frame(frame.stream_id)).await?;
                                continue;
                            }
                            if let Err(refusal) = commands.admit(frame.stream_id, call) {
                                self.client.send(refusal.frame(frame.stream_id)).await?;
                                if self.execution.complete() { return Ok(()); }
                            }
                        }

                        Some(frame::Body::Invoke(Invoke { op: Some(invoke::Op::SurfaceLifecycle(event)) })) => {
                            let identity = event.instance.as_ref().map(omega_proto::instance::InstanceKey::try_from).transpose();
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
                            let identity = event.instance.as_ref().map(omega_proto::instance::InstanceKey::try_from).transpose();
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
                            let identity = match remove.instance.as_ref().map(omega_proto::instance::InstanceKey::try_from).transpose() {
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
                            self.client.send(omega_proto::Refusal::unimplemented("unsupported plugin operation").frame(frame.stream_id)).await?;
                        }
                        Some(frame::Body::Event(event)) => {
                            for (kind, reaction) in &reactions {
                                if event.kind == *kind as i32 {
                                    reaction.fire(&event);
                                }
                            }
                        }

                        Some(frame::Body::Result(answer)) => {
                            if self.effects.answer(frame.stream_id, &answer)? { self.storage_completions(&mut instances).await?; continue; }
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

    async fn storage_completions(&mut self, instances: &mut [Instance]) -> Result<(), Error> {
        for identity in self.context.storage().completions() {
            for instance in instances.iter_mut() {
                if instance.identity == identity {
                    instance.storage_changed();
                }
            }
        }
        self.publish_all(instances).await
    }

    async fn publish_all(&mut self, instances: &mut [Instance]) -> Result<(), Error> {
        for instance in instances {
            if self.publications.len() >= 256 {
                break;
            }
            let Some(view) = instance.changed(&self.context) else {
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
