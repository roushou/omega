//! Everything the daemon knows about one unit.

use std::collections::HashMap;

use tokio::sync::mpsc;

use omega_proto::UnitName;
use omega_proto::omega::{UnitPhase, UnitStatus, Value};

use crate::manifest::UnitManifest;
use crate::shutdown::Shutdown;
use crate::units::lifecycle::{Lifecycle, Transition};
use crate::units::session::Request;
use crate::units::token::UnitToken;

/// The two things that can be asked of a running unit.
#[derive(Debug, Clone)]
pub struct UnitControl {
    /// Stop supervising: the document no longer wants this unit.
    pub stop: Shutdown,
    /// Cycle the process: the document still wants it, just not this
    /// instance. The count is what a restart increments.
    pub cycle: tokio::sync::watch::Sender<u64>,
}

/// One unit, and every fact about it that outlives a function call.
///
/// These facts used to live in five maps behind five locks — the manifests,
/// the tokens, the supervision handles, the sessions, and the phases — each
/// keyed by the same name and each describing the same thing. Asking "what do
/// we know about this unit" had no single answer, and every new fact wanted a
/// sixth map.
#[derive(Debug)]
pub struct UnitRecord {
    pub name: UnitName,
    /// What the build produced for it. Absent for a unit this build does not
    /// contain but something still refers to.
    pub manifest: Option<UnitManifest>,
    pub lifecycle: Lifecycle,
    /// The token of the current spawn, and the pid it was bound to on first
    /// use. Identity is the pair: a pid is recycled, a token is not.
    pub token: Option<UnitToken>,
    pub pid: Option<i32>,
    /// Present while the supervisor is running it.
    pub control: Option<UnitControl>,
    /// Present while the unit holds a session.
    pub session: Option<mpsc::Sender<Request>>,
    /// The settings the document gave it, as the running process was told
    /// them. Construction rather than state: a plugin's fields are built out
    /// of these, so changing them means running the unit again.
    pub config: HashMap<String, Value>,
    /// Somebody has taken this unit's place while they work on it. The
    /// supervisor is not running it and must not start: the built binary and
    /// the one being written would be two processes claiming one name.
    pub adopted: bool,
    /// Spawns after the first.
    pub restarts: u32,
}

impl UnitRecord {
    /// What a unit somebody is developing reports as its detail.
    pub const ADOPTED: &'static str = "adopted for development";

    pub fn new(name: UnitName) -> Self {
        Self {
            name,
            manifest: None,
            lifecycle: Lifecycle::default(),
            token: None,
            pid: None,
            control: None,
            session: None,
            config: HashMap::new(),
            adopted: false,
            restarts: 0,
        }
    }

    pub fn is_connected(&self) -> bool {
        self.session.is_some()
    }

    pub fn is_supervised(&self) -> bool {
        self.control.is_some()
    }

    /// Whether anything is running this unit — the supervisor, or the person
    /// who took it over. What the reconciler asks before starting one.
    pub fn is_held(&self) -> bool {
        self.is_supervised() || self.adopted
    }

    /// Whether this unit may claim to be `token` from `pid`.
    ///
    /// A token that has never been used binds to the first process that
    /// presents it; after that only that process is it.
    pub fn claims(&mut self, pid: i32, token: &str) -> bool {
        if self
            .token
            .as_ref()
            .is_none_or(|held| held.as_str() != token)
        {
            return false;
        }

        match self.pid {
            Some(bound) => bound == pid,
            None => {
                self.pid = Some(pid);
                true
            }
        }
    }

    pub fn apply(&mut self, transition: Transition) -> bool {
        if matches!(transition, Transition::Spawned) && self.lifecycle != Lifecycle::Idle {
            self.restarts += 1;
        }
        self.lifecycle.apply(transition)
    }

    /// What `omega status` and the `units` topic say about it.
    ///
    /// An adopted unit says so: the binary answering to this name is not the
    /// one the last build produced, and a status that did not mention it
    /// would be describing the wrong process.
    pub fn status(&self) -> UnitStatus {
        UnitStatus {
            unit: self.name.to_string(),
            phase: self.phase() as i32,
            restarts: self.restarts,
            last_exit_code: self.lifecycle.exit_code(),
            detail: match self.adopted {
                true => Self::ADOPTED.to_string(),
                false => self.lifecycle.detail().to_string(),
            },
        }
    }

    /// What phase to report.
    ///
    /// An adopted unit's lifecycle describes the supervised process, which
    /// was stopped to make room — so reporting it would describe a process
    /// that no longer exists while another one serves under its name. What is
    /// running is the developer's, and the rule for it is the rule for every
    /// unit: it is running once it has completed the handshake, and starting
    /// until then.
    fn phase(&self) -> UnitPhase {
        if !self.adopted {
            return self.lifecycle.phase(self.is_connected());
        }

        match self.is_connected() {
            true => UnitPhase::Running,
            false => UnitPhase::Starting,
        }
    }
}
