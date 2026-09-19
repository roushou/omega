//! Bounded execution history. Arguments and answers are never retained.
use omega_proto::omega::{CommandExecution, ErrorCode};
use omega_proto::{
    CommandId, Refusal,
    host::{HostId, InvocationId, ProcessId},
};
use std::{
    collections::{BTreeMap, VecDeque},
    sync::{Arc, Mutex},
};
use tokio::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Queued,
    Starting,
    Dispatched,
    Finished,
}

#[derive(Debug)]
struct Entry {
    host: HostId,
    command: CommandId,
    process: Option<ProcessId>,
    phase: Phase,
    admitted: Instant,
    started: Option<Instant>,
    finished: Option<Instant>,
    outcome: Option<Completion>,
}

#[derive(Debug, Default)]
struct State {
    next: u64,
    entries: BTreeMap<InvocationId, Entry>,
    completed: VecDeque<InvocationId>,
}

#[derive(Debug, Default)]
pub(super) struct Invocations(Mutex<State>);

impl Invocations {
    pub(super) fn admit(
        self: &Arc<Self>,
        host: HostId,
        command: CommandId,
        now: Instant,
    ) -> Result<Execution, Refusal> {
        let mut state = self.0.lock().unwrap_or_else(|e| e.into_inner());
        state.next = state
            .next
            .checked_add(1)
            .ok_or_else(|| Refusal::exhausted("invocation identities exhausted"))?;
        let id = InvocationId::try_from(state.next)
            .map_err(|error| Refusal::precondition(error.to_string()))?;
        state.entries.insert(
            id,
            Entry {
                host,
                command,
                process: None,
                phase: Phase::Queued,
                admitted: now,
                started: None,
                finished: None,
                outcome: None,
            },
        );
        Ok(Execution {
            id,
            registry: self.clone(),
            finished: false,
        })
    }

    pub(super) fn activity(&self, host: &HostId, now: Instant) -> Activity {
        let state = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let mut activity = Activity::default();
        for entry in state.entries.values().filter(|entry| &entry.host == host) {
            match entry.phase {
                Phase::Queued => activity.queued += 1,
                Phase::Starting | Phase::Dispatched => activity.active += 1,
                Phase::Finished => {}
            }
        }
        for id in state.completed.iter().rev() {
            let entry = &state.entries[id];
            if &entry.host == host && matches!(entry.outcome, Some(Completion::Refused(_))) {
                activity.failures.push(entry.snapshot(*id, now));
                if activity.failures.len() == 5 {
                    break;
                }
            }
        }
        activity
    }

    pub(super) fn inspect(
        &self,
        host: &HostId,
        command: &str,
        now: Instant,
    ) -> Vec<omega_proto::omega::CommandExecution> {
        let state = self.0.lock().unwrap_or_else(|e| e.into_inner());
        state
            .entries
            .iter()
            .filter(|(_, e)| &e.host == host && e.command.as_str() == command)
            .map(|(id, entry)| entry.snapshot(*id, now))
            .collect()
    }
}

#[derive(Debug, Default)]
pub(super) struct Activity {
    pub queued: u32,
    pub active: u32,
    pub failures: Vec<CommandExecution>,
}

#[derive(Debug, Clone, Copy)]
enum Completion {
    Succeeded,
    Refused(ErrorCode),
    Abandoned,
}

impl Entry {
    fn snapshot(&self, id: InvocationId, now: Instant) -> CommandExecution {
        CommandExecution {
            invocation_id: id.get(),
            command: self.command.to_string(),
            process_id: self.process.map_or(0, ProcessId::get),
            phase: match self.phase {
                Phase::Queued => "queued",
                Phase::Starting => "starting",
                Phase::Dispatched => "dispatched",
                Phase::Finished => "finished",
            }
            .into(),
            queue_ms: self
                .started
                .unwrap_or(self.finished.unwrap_or(now))
                .duration_since(self.admitted)
                .as_millis() as u64,
            execution_ms: self.started.map_or(0, |start| {
                self.finished
                    .unwrap_or(now)
                    .duration_since(start)
                    .as_millis() as u64
            }),
            outcome: match self.outcome {
                None => "",
                Some(Completion::Succeeded) => "ok",
                Some(Completion::Refused(code)) => code.as_str_name(),
                Some(Completion::Abandoned) => "delivery abandoned",
            }
            .into(),
        }
    }
}

#[derive(Debug)]
pub(super) struct Execution {
    pub id: InvocationId,
    registry: Arc<Invocations>,
    finished: bool,
}
impl Execution {
    pub(super) fn starting(&self, now: Instant) {
        let mut state = self.registry.0.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(entry) = state.entries.get_mut(&self.id) {
            entry.phase = Phase::Starting;
            entry.started = Some(now);
        }
    }
    pub(super) fn dispatched(&self, process: ProcessId) {
        let mut state = self.registry.0.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(entry) = state.entries.get_mut(&self.id) {
            entry.phase = Phase::Dispatched;
            entry.process = Some(process);
        }
    }
    pub(super) fn finish(&mut self, result: Result<(), ErrorCode>, now: Instant) {
        self.complete(
            match result {
                Ok(()) => Completion::Succeeded,
                Err(code) => Completion::Refused(code),
            },
            now,
        );
    }

    fn complete(&mut self, outcome: Completion, now: Instant) {
        if self.finished {
            return;
        }
        self.finished = true;
        let mut state = self.registry.0.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(entry) = state.entries.get_mut(&self.id) {
            entry.phase = Phase::Finished;
            entry.finished = Some(now);
            entry.outcome = Some(outcome);
        }
        state.completed.push_back(self.id);
        while state.completed.len() > 64 {
            if let Some(id) = state.completed.pop_front() {
                state.entries.remove(&id);
            }
        }
    }
}
impl Drop for Execution {
    fn drop(&mut self) {
        self.complete(Completion::Abandoned, Instant::now());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test(start_paused = true)]
    async fn activity_counts_current_work_and_retains_only_recent_failures_for_its_host() {
        let registry = Arc::new(Invocations::default());
        let host: HostId = "worker".parse().unwrap();
        let mut pending = Vec::new();
        for index in 0..3 {
            let call = registry
                .admit(host.clone(), "test.run".parse().unwrap(), Instant::now())
                .unwrap();
            if index > 0 {
                call.starting(Instant::now());
            }
            pending.push(call);
        }
        pending[2].dispatched(ProcessId::try_from(1).unwrap());
        for index in 0..8 {
            let mut call = registry
                .admit(
                    host.clone(),
                    format!("test.fail{index}").parse().unwrap(),
                    Instant::now(),
                )
                .unwrap();
            call.finish(Err(ErrorCode::Unavailable), Instant::now());
        }
        let mut other = registry
            .admit(
                "other".parse().unwrap(),
                "test.secret".parse().unwrap(),
                Instant::now(),
            )
            .unwrap();
        other.finish(Err(ErrorCode::InvalidArgument), Instant::now());
        let activity = registry.activity(&host, Instant::now());
        assert_eq!((activity.queued, activity.active), (1, 2));
        assert_eq!(activity.failures.len(), 5);
        assert_eq!(activity.failures[0].command, "test.fail7");
        assert_eq!(activity.failures[4].command, "test.fail3");
        pending[1].finish(Ok(()), Instant::now());
        drop(pending);
        let activity = registry.activity(&host, Instant::now());
        assert_eq!((activity.queued, activity.active), (0, 0));
        assert_eq!(activity.failures.len(), 5);
    }

    #[tokio::test(start_paused = true)]
    async fn tracking_is_bounded_and_separates_queue_and_execution_time() {
        let registry = Arc::new(Invocations::default());
        let host = "worker".parse().unwrap();
        let command = "test.run".parse().unwrap();
        let mut call = registry.admit(host, command, Instant::now()).unwrap();
        tokio::time::advance(std::time::Duration::from_millis(20)).await;
        call.starting(Instant::now());
        call.dispatched(ProcessId::try_from(7).unwrap());
        tokio::time::advance(std::time::Duration::from_millis(30)).await;
        call.finish(Ok(()), Instant::now());
        let view = registry.inspect(&"worker".parse().unwrap(), "test.run", Instant::now());
        assert_eq!(view[0].queue_ms, 20);
        assert_eq!(view[0].execution_ms, 30);
        assert_eq!(view[0].process_id, 7);
        for _ in 0..100 {
            drop(
                registry
                    .admit(
                        "worker".parse().unwrap(),
                        "test.run".parse().unwrap(),
                        Instant::now(),
                    )
                    .unwrap(),
            );
        }
        assert_eq!(
            registry
                .inspect(&"worker".parse().unwrap(), "test.run", Instant::now())
                .len(),
            64
        );
    }
}
