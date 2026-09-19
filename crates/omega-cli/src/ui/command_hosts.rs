//! Command host status uses process facts, independently of surface readiness.

use omega_proto::omega::{CommandHostPhase, CommandHostStatus, HostStart};

use super::{Step, Ui};

impl Ui {
    pub(crate) fn command_hosts(&mut self, hosts: &[CommandHostStatus], detailed: bool) {
        let width = Self::width(hosts.iter().map(|host| host.id.as_str()));

        for host in hosts {
            let phase = CommandHostPhase::try_from(host.phase);
            let step = match phase {
                Ok(CommandHostPhase::Idle) => Step::Idle,
                Ok(CommandHostPhase::Starting) => Step::Starting,
                Ok(CommandHostPhase::Running) => Step::Running,
                Ok(CommandHostPhase::Stopping) => Step::Stopping,
                Ok(CommandHostPhase::Backoff) => Step::Backoff,
                Ok(CommandHostPhase::Failed) => Step::Failed,
                Ok(CommandHostPhase::Unspecified) | Err(_) => Step::Unknown,
            };
            self.step(
                step,
                format!(
                    "{}  {} command host",
                    Self::column(&host.id, width),
                    host.lifetime
                ),
            );
            self.detail(format!(
                "Calls: {} active; {} queued",
                host.active_calls, host.queued_calls
            ));

            if host.processes.is_empty() {
                let retry = host
                    .retry_after_ms
                    .map(|ms| format!("; retry eligible in {ms}ms"))
                    .unwrap_or_default();
                let start = match HostStart::try_from(host.start) {
                    Ok(HostStart::OnDemand) => "Starts on the next call",
                    Ok(HostStart::Eager) => "Starts automatically",
                    Err(_) => "Start policy not reported",
                };
                self.detail(format!("{start}{retry}"));
            } else if detailed || phase != Ok(CommandHostPhase::Running) {
                for process in &host.processes {
                    self.detail(format!("Process {}: {}", process.id, process.phase));
                }
            }

            if !host.startup_error.is_empty() {
                self.detail(format!("Last startup failure: {}", host.startup_error));
            }
            for failure in host
                .recent_failures
                .iter()
                .take(if detailed { 5 } else { 1 })
            {
                self.detail(format!(
                    "{} #{}: {} (queue {}ms; execution {}ms)",
                    failure.command,
                    failure.invocation_id,
                    failure
                        .outcome
                        .strip_prefix("ERROR_CODE_")
                        .unwrap_or(&failure.outcome),
                    failure.queue_ms,
                    failure.execution_ms,
                ));
            }
            if detailed {
                self.detail(format!(
                    "Concurrency: {}; queue capacity: {}",
                    host.concurrency, host.queue_capacity
                ));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use omega_proto::omega::{CommandExecution, CommandProcessStatus};

    struct Fixture;
    impl Fixture {
        fn host() -> CommandHostStatus {
            CommandHostStatus {
                id: "audio-commands".into(),
                lifetime: "persistent".into(),
                phase: CommandHostPhase::Idle as i32,
                start: HostStart::OnDemand as i32,
                concurrency: 2,
                queue_capacity: 32,
                ..Default::default()
            }
        }
    }

    #[test]
    fn idle_starting_running_and_stopping_are_distinct_from_surface_readiness() {
        let (mut ui, transcript) = Ui::recording();
        let mut host = Fixture::host();
        ui.command_hosts(&[host.clone()], false);
        for (phase, label) in [
            (CommandHostPhase::Starting, "starting"),
            (CommandHostPhase::Running, "running"),
            (CommandHostPhase::Stopping, "stopping"),
        ] {
            host.phase = phase as i32;
            host.processes = vec![CommandProcessStatus {
                id: 1,
                phase: label.into(),
            }];
            ui.command_hosts(&[host.clone()], true);
        }
        let output = transcript.err();
        assert!(output.contains("Idle audio-commands"));
        assert!(output.contains("Starts on the next call"));
        assert!(output.contains("Starting audio-commands"));
        assert!(output.contains("Running audio-commands"));
        assert!(output.contains("Stopping audio-commands"));
        assert!(output.contains("Concurrency: 2; queue capacity: 32"));
        assert!(!output.contains("Unknown"));
        assert!(!output.contains("surface"));
        assert!(transcript.out().is_empty());
    }

    #[test]
    fn failure_output_explains_retry_policy_and_recent_execution_failures() {
        let (mut ui, transcript) = Ui::recording();
        let mut host = Fixture::host();
        host.phase = CommandHostPhase::Backoff as i32;
        host.retry_after_ms = Some(450);
        host.startup_error = "executable missing".into();
        host.active_calls = 1;
        host.queued_calls = 2;
        host.recent_failures = vec![CommandExecution {
            command: "audio.volume".into(),
            invocation_id: 7,
            outcome: "ERROR_CODE_OUTCOME_UNKNOWN".into(),
            queue_ms: 4,
            execution_ms: 5000,
            ..Default::default()
        }];
        ui.command_hosts(&[host.clone()], true);
        host.start = HostStart::Eager as i32;
        ui.command_hosts(&[host], true);
        let output = transcript.err();
        assert!(output.contains("Backing off audio-commands"));
        assert!(output.contains("Calls: 1 active; 2 queued"));
        assert!(output.contains("Starts on the next call; retry eligible in 450ms"));
        assert!(output.contains("Starts automatically; retry eligible in 450ms"));
        assert!(output.contains("Last startup failure: executable missing"));
        assert!(output.contains("audio.volume #7: OUTCOME_UNKNOWN (queue 4ms; execution 5000ms)"));
        assert!(transcript.out().is_empty());
    }

    #[test]
    fn missing_or_unrecognized_phase_is_never_reported_as_healthy() {
        let (mut ui, transcript) = Ui::recording();
        for phase in [0, i32::MAX] {
            let mut host = Fixture::host();
            host.phase = phase;
            ui.command_hosts(&[host], false);
        }
        assert_eq!(
            transcript.err().matches("Unknown audio-commands").count(),
            2
        );
    }
}
