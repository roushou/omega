//! Everything the daemon knows about one unit.

use std::collections::HashMap;

use omega_proto::UnitName;
use omega_proto::omega::{UnitPhase, UnitStatus, Value};

use crate::manifest::UnitManifest;
use crate::shutdown::Shutdown;
use crate::units::lifecycle::{Lifecycle, Transition};
use crate::units::session::SessionLink;
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

/// One unit's manifest, identity, supervision handles, session, and lifecycle state.
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
    pub(crate) session: Option<SessionLink>,
    pub(crate) instances:
        std::collections::BTreeMap<omega_proto::instance::InstanceId, super::instance::Instance>,
    /// Settings supplied to the running process at handshake. Changes require restart.
    pub config: HashMap<String, Value>,
    /// Active adoption suppresses supervised spawning.
    pub adopted: bool,
    /// Spawns after the first.
    pub restarts: u32,
}

impl UnitRecord {
    /// Status detail reported by an adopted development process.
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
            instances: Default::default(),
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

    /// Whether supervision or development adoption currently owns the unit.
    pub fn is_held(&self) -> bool {
        self.is_supervised() || self.adopted
    }

    /// Validate token ownership. First use binds the token to the presenting pid.
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

    /// Project lifecycle and adoption state into the units topic.
    pub fn status(&self) -> UnitStatus {
        UnitStatus {
            unit: self.name.to_string(),
            phase: self.phase() as i32,
            restarts: self.restarts,
            last_exit_code: self.lifecycle.exit_code(),
            detail: if self.adopted {
                Self::ADOPTED.to_string()
            } else {
                self.lifecycle.detail().to_string()
            },
        }
    }

    /// Report the adopted process's handshake state while adoption is active;
    /// otherwise report the supervised lifecycle state.
    fn phase(&self) -> UnitPhase {
        if !self.adopted {
            return self.lifecycle.phase(self.is_connected());
        }

        if self.is_connected() {
            UnitPhase::Running
        } else {
            UnitPhase::Starting
        }
    }
}
