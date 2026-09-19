//! Validated daemon assignments and per-session invocation lifecycle.
use crate::{Error, program::ProgramKind};
use omega_proto::{
    Refusal,
    host::{InvocationId, ProcessId},
    omega::HostAssignment,
};

#[derive(Debug, PartialEq, Eq)]
pub(super) enum ExecutionMode {
    Plugin,
    PersistentCommands {
        _process: ProcessId,
    },
    OneShot {
        _process: ProcessId,
        invocation: InvocationId,
        phase: OneShotPhase,
    },
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum OneShotPhase {
    AwaitingCall,
    Running,
    Finished,
}

impl TryFrom<(ProgramKind, Option<HostAssignment>)> for ExecutionMode {
    type Error = Error;
    fn try_from((kind, assignment): (ProgramKind, Option<HostAssignment>)) -> Result<Self, Error> {
        match (kind, assignment) {
            (ProgramKind::Plugin, None) => Ok(Self::Plugin),
            (ProgramKind::Plugin, Some(_)) => Err(Error::invalid(
                "plugins cannot receive command-host assignments",
            )),
            (ProgramKind::Commands, None) => Err(Error::invalid(
                "command hosts must be started by the Omega daemon",
            )),
            (ProgramKind::Commands, Some(assignment)) => {
                let process = ProcessId::try_from(assignment.process_id).map_err(|error| {
                    Error::invalid(format!("invalid host process identity: {error}"))
                })?;
                if assignment.invocation_id == 0 {
                    Ok(Self::PersistentCommands { _process: process })
                } else {
                    let invocation =
                        InvocationId::try_from(assignment.invocation_id).map_err(|error| {
                            Error::invalid(format!("invalid invocation identity: {error}"))
                        })?;
                    Ok(Self::OneShot {
                        _process: process,
                        invocation,
                        phase: OneShotPhase::AwaitingCall,
                    })
                }
            }
        }
    }
}

impl ExecutionMode {
    pub(super) fn admit(&mut self, invocation_id: u64) -> Result<(), Refusal> {
        if let Self::OneShot {
            invocation, phase, ..
        } = self
        {
            if invocation_id != invocation.get() {
                return Err(Refusal::precondition(
                    "invocation does not match the one-shot assignment",
                ));
            }
            if *phase != OneShotPhase::AwaitingCall {
                return Err(Refusal::precondition(
                    "one-shot invocation already admitted",
                ));
            }
            *phase = OneShotPhase::Running;
        }
        Ok(())
    }

    /// Complete the assigned call, including an admission refusal. True means exit after replying.
    pub(super) fn complete(&mut self) -> bool {
        match self {
            Self::OneShot { phase, .. } => {
                *phase = OneShotPhase::Finished;
                true
            }
            Self::Plugin | Self::PersistentCommands { .. } => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assignments_must_match_the_executable_kind_and_have_a_process_identity() {
        assert_eq!(
            ExecutionMode::try_from((ProgramKind::Plugin, None)).unwrap(),
            ExecutionMode::Plugin
        );
        assert!(ExecutionMode::try_from((ProgramKind::Commands, None)).is_err());
        for invocation_id in [0, 42] {
            let assignment = HostAssignment {
                process_id: 1,
                invocation_id,
            };
            assert!(ExecutionMode::try_from((ProgramKind::Plugin, Some(assignment))).is_err());
            assert!(ExecutionMode::try_from((ProgramKind::Commands, Some(assignment))).is_ok());
            assert!(
                ExecutionMode::try_from((
                    ProgramKind::Commands,
                    Some(HostAssignment {
                        process_id: 0,
                        invocation_id
                    })
                ))
                .is_err()
            );
        }
    }

    #[test]
    fn one_shot_accepts_only_its_assignment_once_and_finishes_after_a_terminal_answer() {
        let mut mode = ExecutionMode::try_from((
            ProgramKind::Commands,
            Some(HostAssignment {
                process_id: 1,
                invocation_id: 42,
            }),
        ))
        .unwrap();
        assert!(mode.admit(41).is_err());
        assert!(mode.admit(0).is_err());
        mode.admit(42).unwrap();
        assert!(mode.admit(42).is_err());
        assert!(mode.complete());
        assert!(matches!(
            mode,
            ExecutionMode::OneShot {
                phase: OneShotPhase::Finished,
                ..
            }
        ));
        assert!(mode.admit(42).is_err());
    }

    #[test]
    fn persistent_and_plugin_sessions_accept_multiple_calls_without_exiting() {
        for mut mode in [
            ExecutionMode::Plugin,
            ExecutionMode::try_from((
                ProgramKind::Commands,
                Some(HostAssignment {
                    process_id: 1,
                    invocation_id: 0,
                }),
            ))
            .unwrap(),
        ] {
            for id in [1, 2] {
                mode.admit(id).unwrap();
                assert!(!mode.complete());
            }
        }
    }
}
