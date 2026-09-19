use std::{num::NonZeroUsize, time::Duration};

use super::HostId;
use crate::omega::{CommandHostConfig, HostLifetime, HostStart};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartPolicy {
    Eager,
    OnDemand,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lifetime {
    Persistent(StartPolicy),
    OneShot,
}

/// Finite provider-wide bounds. Deadlines are measured independently per phase.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionPolicy {
    pub concurrency: NonZeroUsize,
    pub queue_capacity: usize,
    pub queue_timeout: Duration,
    pub startup_timeout: Duration,
    pub execution_timeout: Duration,
}

impl Default for ExecutionPolicy {
    fn default() -> Self {
        Self::serial()
    }
}

impl ExecutionPolicy {
    /// Caller ceiling: three bounded phases and process cleanup. Provider deadlines
    /// usually terminate a call much sooner; expiration never permits replay.
    pub const CALL_TIMEOUT: Duration = Duration::from_secs(905);

    /// One running invocation, up to 32 waiting, with 5s queue, startup, and execution deadlines.
    pub fn serial() -> Self {
        Self::bounded(NonZeroUsize::MIN)
    }
    pub fn bounded(concurrency: NonZeroUsize) -> Self {
        Self {
            concurrency,
            queue_capacity: 32,
            queue_timeout: Duration::from_secs(5),
            startup_timeout: Duration::from_secs(5),
            execution_timeout: Duration::from_secs(5),
        }
    }
    pub fn validate(&self) -> Result<(), HostPolicyError> {
        if self.concurrency.get() > 64 || self.queue_capacity > 1024 {
            return Err(HostPolicyError::Invalid(
                "concurrency must be at most 64 and queue capacity at most 1024",
            ));
        }
        for timeout in [
            self.queue_timeout,
            self.startup_timeout,
            self.execution_timeout,
        ] {
            if timeout < Duration::from_millis(1)
                || timeout > Duration::from_secs(300)
                || timeout.subsec_nanos() % 1_000_000 != 0
            {
                return Err(HostPolicyError::Invalid(
                    "deadlines must be whole milliseconds between 1ms and 5min",
                ));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostPolicy {
    pub id: HostId,
    pub lifetime: Lifetime,
    pub execution: ExecutionPolicy,
}

#[derive(Debug, thiserror::Error)]
pub enum HostPolicyError {
    #[error(transparent)]
    Identity(#[from] crate::IdentError),
    #[error("invalid command host policy: {0}")]
    Invalid(&'static str),
}

impl TryFrom<&CommandHostConfig> for HostPolicy {
    type Error = HostPolicyError;
    fn try_from(value: &CommandHostConfig) -> Result<Self, Self::Error> {
        let start = match HostStart::try_from(value.start) {
            Ok(HostStart::Eager) => StartPolicy::Eager,
            Ok(HostStart::OnDemand) => StartPolicy::OnDemand,
            Err(_) => return Err(HostPolicyError::Invalid("unknown start policy")),
        };
        let lifetime = match HostLifetime::try_from(value.lifetime) {
            Ok(HostLifetime::Persistent) => Lifetime::Persistent(start),
            Ok(HostLifetime::OneShot) if start == StartPolicy::OnDemand => Lifetime::OneShot,
            Ok(HostLifetime::OneShot) => {
                return Err(HostPolicyError::Invalid(
                    "one-shot hosts start only for invocations",
                ));
            }
            Err(_) => return Err(HostPolicyError::Invalid("unknown lifetime")),
        };
        let execution = ExecutionPolicy {
            concurrency: NonZeroUsize::new(value.concurrency as usize)
                .ok_or(HostPolicyError::Invalid("concurrency must be positive"))?,
            queue_capacity: value.queue_capacity as usize,
            queue_timeout: Duration::from_millis(value.queue_timeout_ms),
            startup_timeout: Duration::from_millis(value.startup_timeout_ms),
            execution_timeout: Duration::from_millis(value.execution_timeout_ms),
        };
        execution.validate()?;
        Ok(Self {
            id: value.id.parse()?,
            lifetime,
            execution,
        })
    }
}

impl TryFrom<HostPolicy> for CommandHostConfig {
    type Error = HostPolicyError;
    fn try_from(value: HostPolicy) -> Result<Self, Self::Error> {
        value.execution.validate()?;
        let (lifetime, start) = match value.lifetime {
            Lifetime::Persistent(StartPolicy::Eager) => {
                (HostLifetime::Persistent, HostStart::Eager)
            }
            Lifetime::Persistent(StartPolicy::OnDemand) => {
                (HostLifetime::Persistent, HostStart::OnDemand)
            }
            Lifetime::OneShot => (HostLifetime::OneShot, HostStart::OnDemand),
        };
        Ok(Self {
            id: value.id.to_string(),
            lifetime: lifetime as i32,
            start: start as i32,
            concurrency: value.execution.concurrency.get() as u32,
            queue_capacity: value.execution.queue_capacity as u32,
            queue_timeout_ms: value.execution.queue_timeout.as_millis() as u64,
            startup_timeout_ms: value.execution.startup_timeout.as_millis() as u64,
            execution_timeout_ms: value.execution.execution_timeout.as_millis() as u64,
            settings: Default::default(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn policies_round_trip_and_reject_unbounded_or_conflicting_settings() {
        let policy = HostPolicy {
            id: "audio".parse().unwrap(),
            lifetime: Lifetime::OneShot,
            execution: ExecutionPolicy::serial(),
        };
        let mut wire = CommandHostConfig::try_from(policy.clone()).unwrap();
        assert_eq!(HostPolicy::try_from(&wire).unwrap(), policy);
        wire.start = HostStart::Eager as i32;
        assert!(HostPolicy::try_from(&wire).is_err());
        wire.start = HostStart::OnDemand as i32;
        wire.execution_timeout_ms = 0;
        assert!(HostPolicy::try_from(&wire).is_err());
    }
}
