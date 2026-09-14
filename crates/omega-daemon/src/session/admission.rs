//! Authenticate plugin and operator peers.
//! Plugin identity requires peer credentials and a daemon-issued spawn token.
//! Operators must have the daemon owner's UID. Other peers are refused.
//! Capabilities come only from the daemon's stored manifest.

use std::collections::HashMap;

use omega_proto::Manifest;
use omega_proto::Refusal;
use omega_proto::omega::{Capability, SurfaceKind};
use omega_proto::{SurfaceId, UnitName};

use crate::process::Identity;
use crate::refusal::Refusable;

/// What kind of peer an op is served to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Renderer,
    /// A unit the supervisor spawned.
    Unit,
    /// The user who owns the daemon.
    Operator,
}

/// An admitted peer.
#[derive(Debug)]
pub struct Peer {
    id: PeerId,
    grants: Grants,
}

#[derive(Debug)]
enum PeerId {
    /// A unit the supervisor spawned and vouches for.
    Unit(UnitName),
    /// The daemon's own user, holding no unit's grants.
    Operator(i32),
}

impl Peer {
    pub fn unit(name: UnitName, manifest: &Manifest) -> Result<Self, Refusal> {
        Ok(Self {
            grants: Grants::of(manifest)?,
            id: PeerId::Unit(name),
        })
    }

    /// Authenticate a tokenless operator using the daemon owner's peer UID.
    pub fn operator(pid: i32, uid: u32) -> Result<Self, Refusal> {
        if uid != Identity::uid() {
            return Err(Refusal::unauthenticated(
                "not a unit of this daemon, and not its owner",
            ));
        }

        Ok(Self {
            id: PeerId::Operator(pid),
            // An operator holds no unit's capabilities: it may cycle a unit,
            // not act as one.
            grants: Grants::none(),
        })
    }

    pub fn role(&self) -> Role {
        match self.id {
            PeerId::Unit(_) => Role::Unit,
            PeerId::Operator(_) => Role::Operator,
        }
    }

    /// The unit this peer is, if it is one.
    pub fn unit_name(&self) -> Option<&UnitName> {
        match &self.id {
            PeerId::Unit(name) => Some(name),
            PeerId::Operator(_) => None,
        }
    }

    /// How the peer is named in logs and in `Welcome`.
    pub fn label(&self) -> String {
        match &self.id {
            PeerId::Unit(name) => name.to_string(),
            PeerId::Operator(pid) => format!("operator-{pid}"),
        }
    }

    pub fn grants(&self) -> &Grants {
        &self.grants
    }
}

/// What a peer may do, resolved once at the handshake from the daemon's copy
/// of the manifest.
#[derive(Debug)]
pub struct Grants {
    capabilities: Vec<Capability>,
    surfaces: HashMap<SurfaceId, SurfaceKind>,
}

impl Grants {
    /// The grants a manifest describes. The manifest was validated when it was
    /// loaded, so an unknown capability here is a daemon bug, not a unit's.
    fn of(manifest: &Manifest) -> Result<Self, Refusal> {
        let capabilities = manifest.granted().map_err(|e| e.refusal())?;

        let surfaces = manifest
            .surfaces
            .iter()
            .map(|surface| {
                Ok((
                    surface.surface_id().map_err(|e| e.refusal())?,
                    surface.declared().map_err(|e| e.refusal())?,
                ))
            })
            .collect::<Result<_, Refusal>>()?;

        Ok(Self {
            capabilities,
            surfaces,
        })
    }

    fn none() -> Self {
        Self {
            capabilities: Vec::new(),
            surfaces: HashMap::new(),
        }
    }

    pub fn holds(&self, capability: Capability) -> bool {
        self.capabilities.contains(&capability)
    }

    /// The kind a surface was declared as, if this peer declared it at all.
    pub fn surface(&self, id: &SurfaceId) -> Option<SurfaceKind> {
        self.surfaces.get(id).copied()
    }

    /// The capability list sent in `Welcome`, as wire values.
    pub fn wire(&self) -> Vec<i32> {
        self.capabilities.iter().map(|c| *c as i32).collect()
    }
}
