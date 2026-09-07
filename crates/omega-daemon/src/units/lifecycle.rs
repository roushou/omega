//! Where a unit is in its life.
//!
//! The phases were always here — `UnitPhase` is in the schema, and the
//! supervisor reported them from half a dozen places inside its loop. What
//! was missing was the state: the phase lived only in the report, so the code
//! that needed to know what was happening had to read its own log to find
//! out, and nothing stopped two reports from disagreeing.
//!
//! Here the state is the state, transitions are the only way to change it,
//! and what the `units` topic says is a projection of it.

use omega_wire::omega::UnitPhase;

/// What is true of a unit's process.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Lifecycle {
    /// Built and known; nothing is running.
    #[default]
    Idle,
    /// A process was spawned for it.
    Starting,
    /// The process is up.
    Running,
    /// It exited, and the supervisor is waiting out the backoff.
    Restarting { exit: i32, detail: String },
    /// It could not be spawned at all.
    Failed { detail: String },
    /// It was stopped on purpose, and will not come back on its own.
    Stopped,
}

/// A thing that happened to a unit. Transitions are named for the event, not
/// the destination: the state machine decides where an event leads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Transition {
    /// A process was spawned.
    Spawned,
    /// The spawn itself failed.
    Unspawnable(String),
    /// The process exited on its own.
    Exited { code: i32, detail: String },
    /// The supervisor is done with it: stopped, or the daemon is going down.
    Stopped,
}

impl Lifecycle {
    /// Apply an event. Returns whether the state moved, so a caller can
    /// publish only what changed.
    pub fn apply(&mut self, transition: Transition) -> bool {
        let next = match (&*self, transition) {
            // A stopped unit stays stopped until something spawns it again.
            (Self::Stopped, Transition::Spawned) => Self::Starting,
            (Self::Stopped, _) => return false,

            (_, Transition::Spawned) => Self::Starting,
            (_, Transition::Unspawnable(detail)) => Self::Failed { detail },
            (_, Transition::Exited { code, detail }) => Self::Restarting { exit: code, detail },
            (_, Transition::Stopped) => Self::Stopped,
        };

        let moved = next != *self;
        *self = next;
        moved
    }

    /// Whether a process is up. A connected unit is running; a spawned one
    /// that has not checked in yet is only starting.
    pub fn phase(&self, connected: bool) -> UnitPhase {
        match self {
            Self::Idle => UnitPhase::Unspecified,
            Self::Starting => UnitPhase::Starting,
            // Spawning a process is not the same as having a unit: until it
            // completes the handshake the daemon has not vouched for it, and
            // saying "running" would be claiming more than it knows.
            Self::Running if connected => UnitPhase::Running,
            Self::Running => UnitPhase::Starting,
            Self::Restarting { .. } => UnitPhase::Restarting,
            Self::Failed { .. } => UnitPhase::Failed,
            Self::Stopped => UnitPhase::Stopped,
        }
    }

    /// The last failure, for a human reading `omega status`.
    pub fn detail(&self) -> &str {
        match self {
            Self::Restarting { detail, .. } | Self::Failed { detail } => detail,
            _ => "",
        }
    }

    /// The exit code of the last run, or -1 when it was signalled or never
    /// ran.
    pub fn exit_code(&self) -> i32 {
        match self {
            Self::Restarting { exit, .. } => *exit,
            Self::Failed { .. } => -1,
            _ => 0,
        }
    }

    pub fn is_stopped(&self) -> bool {
        matches!(self, Self::Stopped)
    }
}

/// The process is up and has completed its handshake.
impl Lifecycle {
    pub fn connected(&mut self) -> bool {
        match self {
            // A unit connects while the supervisor thinks it is starting;
            // that is the handshake completing.
            Self::Starting | Self::Running => {
                let moved = *self != Self::Running;
                *self = Self::Running;
                moved
            }
            // Anything else is a peer the supervisor is not running: its
            // session is admitted on its own terms, and its lifecycle is not
            // this daemon's to invent.
            _ => false,
        }
    }
}
