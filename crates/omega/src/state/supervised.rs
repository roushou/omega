//! The supervisor's report on every unit it runs.

use crate::state::Units;

pub use omega_proto::omega::UnitPhase;

/// What the daemon knows about one unit.
#[derive(Debug, Clone, PartialEq)]
pub struct UnitReport {
    unit: String,
    phase: UnitPhase,
    restarts: u32,
    last_exit_code: i32,
    detail: String,
}

impl UnitReport {
    fn of(status: omega_proto::omega::UnitStatus) -> Self {
        Self {
            phase: UnitPhase::try_from(status.phase).unwrap_or(UnitPhase::Unspecified),
            unit: status.unit,
            restarts: status.restarts,
            last_exit_code: status.last_exit_code,
            detail: status.detail,
        }
    }

    pub fn unit(&self) -> &str {
        &self.unit
    }

    pub fn phase(&self) -> UnitPhase {
        self.phase
    }

    /// Spawns after the first. Nought is a unit that has never fallen over.
    pub fn restarts(&self) -> u32 {
        self.restarts
    }

    /// How it last exited, or `None` where it was signalled or never ran.
    pub fn last_exit_code(&self) -> Option<i32> {
        match self.last_exit_code {
            -1 => None,
            code => Some(code),
        }
    }

    /// The last failure, for a person to read.
    pub fn detail(&self) -> Option<&str> {
        match self.detail.is_empty() {
            true => None,
            false => Some(&self.detail),
        }
    }

    pub fn is_running(&self) -> bool {
        self.phase == UnitPhase::Running
    }

    /// Failed outright, or exited and waiting out a backoff. Both are a unit
    /// that is not doing its job, which is the question a health widget asks.
    pub fn is_troubled(&self) -> bool {
        matches!(self.phase, UnitPhase::Failed | UnitPhase::Restarting)
    }
}

impl Units {
    pub fn all(&self) -> Vec<UnitReport> {
        self.read()
            .map(|state| state.units.into_iter().map(UnitReport::of).collect())
            .unwrap_or_default()
    }

    pub fn of(&self, unit: &str) -> Option<UnitReport> {
        self.all().into_iter().find(|report| report.unit == unit)
    }

    /// Everything that is not doing its job.
    pub fn troubled(&self) -> Vec<UnitReport> {
        self.all()
            .into_iter()
            .filter(UnitReport::is_troubled)
            .collect()
    }
}
