//! Startup results and restart eligibility for one configured provider.

use omega_proto::Refusal;
use omega_proto::host::{Lifetime, StartPolicy};
use omega_proto::omega::{CommandHostPhase, CommandProcessStatus};
use tokio::time::Instant;

#[derive(Debug, Default)]
pub(super) struct Startup {
    backoff: crate::supervisor::Backoff,
    last: Option<Instant>,
    next: Option<Instant>,
    error: Option<String>,
}

impl Startup {
    pub(super) fn reserve(&mut self, now: Instant) -> Result<(), Refusal> {
        if self.next.is_some_and(|next| now < next) {
            return Err(Refusal::unavailable(
                "command host is waiting for restart backoff",
            ));
        }
        if self
            .last
            .is_some_and(|last| now.duration_since(last) >= crate::supervisor::Backoff::HEALTHY)
        {
            self.backoff.reset();
        }
        self.last = Some(now);
        self.next = Some(now + self.backoff.delay());
        Ok(())
    }

    pub(super) fn failed(&mut self, message: &str) {
        self.error = Some(message.chars().take(512).collect());
    }

    pub(super) fn succeeded(&mut self) {
        self.error = None;
    }

    pub(super) fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub(super) fn retry_after(&self, now: Instant) -> Option<u64> {
        self.next
            .filter(|next| *next > now)
            .map(|next| next.duration_since(now).as_nanos().div_ceil(1_000_000) as u64)
    }

    pub(super) fn phase(
        &self,
        processes: &[CommandProcessStatus],
        lifetime: Lifetime,
        now: Instant,
    ) -> CommandHostPhase {
        if processes.iter().any(|process| process.phase == "running") {
            CommandHostPhase::Running
        } else if processes.iter().any(|process| process.phase == "starting") {
            CommandHostPhase::Starting
        } else if !processes.is_empty() {
            CommandHostPhase::Stopping
        } else if self.retry_after(now).is_some() {
            CommandHostPhase::Backoff
        } else if self.error.is_some() {
            CommandHostPhase::Failed
        } else if lifetime == Lifetime::Persistent(StartPolicy::Eager) {
            CommandHostPhase::Starting
        } else {
            CommandHostPhase::Idle
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test(start_paused = true)]
    async fn eligibility_and_last_failure_are_independent_of_process_readiness() {
        let mut startup = Startup::default();
        let lifetime = Lifetime::Persistent(StartPolicy::OnDemand);
        assert_eq!(
            startup.phase(&[], lifetime, Instant::now()),
            CommandHostPhase::Idle
        );
        startup.reserve(Instant::now()).unwrap();
        startup.failed("cannot spawn");
        assert_eq!(
            startup.phase(&[], lifetime, Instant::now()),
            CommandHostPhase::Backoff
        );
        assert!(startup.reserve(Instant::now()).is_err());
        assert_eq!(startup.error(), Some("cannot spawn"));
        tokio::time::advance(Duration::from_secs(1)).await;
        assert!(startup.retry_after(Instant::now()).is_none());
        assert_eq!(
            startup.phase(&[], lifetime, Instant::now()),
            CommandHostPhase::Failed
        );
        startup.succeeded();
        assert_eq!(
            startup.phase(&[], lifetime, Instant::now()),
            CommandHostPhase::Idle
        );
        assert!(startup.error().is_none());
    }

    #[test]
    fn active_processes_take_precedence_and_failure_text_is_bounded() {
        let mut startup = Startup::default();
        startup.failed(&"é".repeat(1000));
        assert_eq!(startup.error().unwrap().chars().count(), 512);
        for (phase, expected) in [
            ("running", CommandHostPhase::Running),
            ("starting", CommandHostPhase::Starting),
            ("stopping", CommandHostPhase::Stopping),
        ] {
            let processes = [CommandProcessStatus {
                id: 1,
                phase: phase.into(),
            }];
            assert_eq!(
                startup.phase(&processes, Lifetime::OneShot, Instant::now()),
                expected
            );
        }
    }
}
