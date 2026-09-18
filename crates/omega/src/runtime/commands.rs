//! Session-owned command handlers, admission, and concurrent execution.

use std::{collections::HashMap, sync::Arc};

use omega_proto::omega::{CallCommand, ErrorCode};
use omega_proto::{CommandAnswer, CommandId, Frame, Refusal, Values};
use tokio::task::{Id, JoinSet};

use super::context::Context;
use crate::{
    command::Args,
    error::Error,
    plugin::{Plugin, registry::CalledCommand},
};

pub(super) struct Commands {
    handlers: HashMap<CommandId, Arc<dyn CalledCommand>>,
    tasks: JoinSet<Result<CommandAnswer, Error>>,
    streams: HashMap<Id, u64>,
    poisoned: bool,
}

pub(super) enum Completion {
    Answer(Box<Frame>),
    Failed { replies: Vec<Frame>, error: Error },
}

impl Commands {
    pub(super) const LIMIT: usize = 64;

    pub(super) fn new(
        plugin: &Plugin,
        context: &Context,
        settings: &Values,
    ) -> Result<Self, Error> {
        let handlers = plugin
            .commands()
            .iter()
            .map(|entry| {
                Ok((
                    entry
                        .name
                        .parse::<CommandId>()
                        .map_err(|error| Error::invalid(error.to_string()))?,
                    entry.build(context, settings),
                ))
            })
            .collect::<Result<_, Error>>()?;
        Ok(Self {
            handlers,
            tasks: JoinSet::new(),
            streams: HashMap::new(),
            poisoned: false,
        })
    }

    pub(super) fn admit(&mut self, stream: u64, call: CallCommand) -> Result<(), Refusal> {
        if self.poisoned {
            return Err(Refusal::unavailable("command execution has ended"));
        }
        if self.tasks.len() >= Self::LIMIT {
            return Err(Refusal::exhausted("command completion capacity exhausted"));
        }
        let id = call
            .command
            .parse::<CommandId>()
            .map_err(|error| Refusal::invalid(error.to_string()))?;
        let command = self
            .handlers
            .get(&id)
            .cloned()
            .ok_or_else(|| Refusal::invalid(format!("no command {}", call.command)))?;
        let task = self
            .tasks
            .spawn(async move { command.call(Args::new(call.args)).await });
        self.streams.insert(task.id(), stream);
        Ok(())
    }

    /// Cancellation leaves pending tasks and their reply streams owned by this session.
    pub(super) async fn next(&mut self) -> Completion {
        let Some(answer) = self.tasks.join_next_with_id().await else {
            return std::future::pending().await;
        };
        match answer {
            Ok((task, completion)) => {
                let Some(stream) = self.streams.remove(&task) else {
                    return self.fail(std::io::Error::other("command task has no reply stream"));
                };
                Completion::Answer(Box::new(match completion {
                    Ok(answer) => Frame::reply(stream, answer.into_outcome()),
                    Err(error) => error.refusal().frame(stream),
                }))
            }
            Err(error) => self.fail(std::io::Error::other(error)),
        }
    }

    fn fail(&mut self, error: std::io::Error) -> Completion {
        self.poisoned = true;
        self.tasks.abort_all();
        let refusal = Refusal::new(
            ErrorCode::OutcomeUnknown,
            "command handler failed; plugin session is ending and effects may have executed",
        );
        let replies = self
            .streams
            .drain()
            .map(|(_, stream)| refusal.frame(stream))
            .collect();
        Completion::Failed {
            replies,
            error: Error::Runtime(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use omega_proto::omega::{frame, result};
    use std::{future::Future, pin::Pin};

    struct Handler {
        panic: bool,
    }

    impl CalledCommand for Handler {
        fn call(
            self: Arc<Self>,
            _: Args,
        ) -> Pin<Box<dyn Future<Output = Result<CommandAnswer, Error>> + Send>> {
            Box::pin(async move {
                if self.panic {
                    panic!("fixture panic");
                }
                std::future::pending().await
            })
        }
    }

    impl Commands {
        fn fixture() -> Self {
            Self {
                handlers: [
                    (
                        "wait".parse().unwrap(),
                        Arc::new(Handler { panic: false }) as Arc<dyn CalledCommand>,
                    ),
                    (
                        "panic".parse().unwrap(),
                        Arc::new(Handler { panic: true }) as Arc<dyn CalledCommand>,
                    ),
                ]
                .into(),
                tasks: JoinSet::new(),
                streams: HashMap::new(),
                poisoned: false,
            }
        }

        fn call(&mut self, stream: u64, command: &str) -> Result<(), Refusal> {
            self.admit(
                stream,
                CallCommand {
                    command: command.into(),
                    args: vec![],
                },
            )
        }
    }

    #[tokio::test]
    async fn panic_refuses_all_pending_streams_and_closes_admission() {
        let mut commands = Commands::fixture();
        commands.call(1, "wait").unwrap();
        commands.call(3, "panic").unwrap();
        let Completion::Failed { replies, .. } = commands.next().await else {
            panic!("expected fail-stop completion");
        };
        let mut streams = Vec::new();
        for reply in replies {
            streams.push(reply.stream_id);
            let Some(frame::Body::Result(answer)) = reply.body else {
                panic!("expected result")
            };
            let Some(result::Outcome::Error(error)) = answer.outcome else {
                panic!("expected refusal")
            };
            assert_eq!(error.code, ErrorCode::OutcomeUnknown as i32);
        }
        streams.sort();
        assert_eq!(streams, [1, 3]);
        assert!(commands.streams.is_empty());
        assert!(commands.call(5, "wait").is_err());
    }

    #[tokio::test]
    async fn cancelling_completion_wait_keeps_admitted_calls() {
        let mut commands = Commands::fixture();
        commands.call(1, "wait").unwrap();
        tokio::select! {
            biased;
            _ = commands.next() => panic!("unfinished call completed"),
            _ = std::future::ready(()) => {}
        }
        assert_eq!(commands.tasks.len(), 1);
        assert_eq!(commands.streams.values().copied().collect::<Vec<_>>(), [1]);
        commands.call(3, "panic").unwrap();
        let Completion::Failed { replies, .. } = commands.next().await else {
            panic!("expected failure")
        };
        assert_eq!(replies.len(), 2);
    }
}
