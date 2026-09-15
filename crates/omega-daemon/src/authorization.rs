//! Authorization facts resolved from the daemon's stored manifest.

use std::collections::HashMap;

use omega_proto::omega::{Capability, SurfaceKind};
use omega_proto::{Manifest, Refusal, SurfaceId};

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
    pub(crate) fn of(manifest: &Manifest) -> Result<Self, Refusal> {
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

    pub(crate) fn none() -> Self {
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
