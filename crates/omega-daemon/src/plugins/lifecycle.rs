//! Where a plugin is in its life.
//!
//! Transitions are the only way to change lifecycle state. The `plugins` topic
//! projects that state so supervision and observers cannot report different phases.

use omega_proto::omega::PluginPhase;

/// What is true of a plugin's process.
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
    /// Explicitly stopped; automatic restart is disabled.
    Stopped,
}

/// A thing that happened to a plugin. Transitions are named for the event, not
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
            // A stopped plugin stays stopped until something spawns it again.
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

    /// Whether a process is up. A connected plugin is running; a spawned one
    /// that has not checked in yet is only starting.
    pub fn phase(&self, connected: bool) -> PluginPhase {
        match self {
            Self::Idle => PluginPhase::Unspecified,
            Self::Starting => PluginPhase::Starting,
            // Report starting until the process completes its handshake.
            Self::Running if connected => PluginPhase::Running,
            Self::Running => PluginPhase::Starting,
            Self::Restarting { .. } => PluginPhase::Restarting,
            Self::Failed { .. } => PluginPhase::Failed,
            Self::Stopped => PluginPhase::Stopped,
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
            // A plugin connects while the supervisor thinks it is starting;
            // that is the handshake completing.
            Self::Starting | Self::Running => {
                let moved = *self != Self::Running;
                *self = Self::Running;
                moved
            }
            // Unsupervised peers do not acquire a supervised lifecycle.
            _ => false,
        }
    }
}
