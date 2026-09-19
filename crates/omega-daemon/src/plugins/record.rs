//! Everything the daemon knows about one plugin.

use std::collections::HashMap;

use omega_proto::PluginName;
use omega_proto::omega::{PluginPhase, PluginStatus, Value};

use crate::manifest::PluginManifest;
use crate::plugins::lifecycle::{Lifecycle, Transition};
use crate::plugins::session::SessionLink;
use crate::process::SpawnIdentity;
use crate::shutdown::Shutdown;

/// The two things that can be asked of a running plugin.
#[derive(Debug, Clone)]
pub struct PluginControl {
    /// Stop supervising: the document no longer wants this plugin.
    pub stop: Shutdown,
    /// Cycle the process: the document still wants it, just not this
    /// instance. The count is what a restart increments.
    pub cycle: tokio::sync::watch::Sender<u64>,
}

/// One plugin's manifest, identity, supervision handles, session, and lifecycle state.
#[derive(Debug)]
pub struct PluginRecord {
    pub name: PluginName,
    /// What the build produced for it. Absent for a plugin this build does not
    /// contain but something still refers to.
    pub manifest: Option<PluginManifest>,
    pub lifecycle: Lifecycle,
    pub(crate) identity: Option<SpawnIdentity>,
    /// Present while the supervisor is running it.
    pub control: Option<PluginControl>,
    /// Present while the plugin holds a session.
    pub(crate) session: Option<SessionLink>,
    pub(crate) instances:
        std::collections::BTreeMap<omega_proto::instance::InstanceId, super::instance::Instance>,
    pub(crate) placements: std::collections::BTreeSet<crate::hub::SurfaceRef>,
    /// Settings supplied to the running process at handshake. Changes require restart.
    pub config: HashMap<String, Value>,
    /// Active adoption suppresses supervised spawning.
    pub adopted: bool,
    /// Spawns after the first.
    pub restarts: u32,
}

impl PluginRecord {
    /// Status detail reported by an adopted development process.
    pub const ADOPTED: &'static str = "adopted for development";

    pub fn new(name: PluginName) -> Self {
        Self {
            name,
            manifest: None,
            lifecycle: Lifecycle::default(),
            identity: None,
            control: None,
            session: None,
            instances: Default::default(),
            placements: Default::default(),
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

    /// Whether supervision or development adoption currently owns the plugin.
    pub fn is_held(&self) -> bool {
        self.is_supervised() || self.adopted
    }

    /// Validate token ownership. First use binds the token to the presenting pid.
    pub fn claims(&mut self, pid: i32, token: &str) -> bool {
        self.identity
            .as_mut()
            .is_some_and(|identity| identity.claims(pid, token))
    }

    pub fn apply(&mut self, transition: Transition) -> bool {
        if matches!(transition, Transition::Spawned) && self.lifecycle != Lifecycle::Idle {
            self.restarts += 1;
        }
        self.lifecycle.apply(transition)
    }

    /// Project lifecycle and adoption state into the plugins topic.
    pub fn status(&self) -> PluginStatus {
        PluginStatus {
            plugin: self.name.to_string(),
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
    fn phase(&self) -> PluginPhase {
        if !self.adopted {
            return self.lifecycle.phase(self.is_connected());
        }

        if self.is_connected() {
            PluginPhase::Running
        } else {
            PluginPhase::Starting
        }
    }
}
