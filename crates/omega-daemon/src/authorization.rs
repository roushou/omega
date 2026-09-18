//! Authorization facts resolved from the daemon's stored manifest.

use std::collections::HashMap;

use omega_proto::omega::{Capability, SurfaceKind};
use omega_proto::{Manifest, Refusal, SurfaceId};

use crate::refusal::Refusable;

/// What kind of peer an op is served to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Renderer,
    /// A plugin the supervisor spawned.
    Plugin,
    /// The user who owns the daemon.
    Operator,
}

/// What a peer may do, resolved once at the handshake from the daemon's copy
/// of the manifest.
#[derive(Debug)]
pub struct Grants {
    commands: std::collections::BTreeMap<omega_proto::CommandAddress, Vec<u8>>,
    storage: std::collections::BTreeMap<omega_proto::storage::StorageId, bool>,
    capabilities: Vec<Capability>,
    surfaces: HashMap<SurfaceId, SurfaceKind>,
}

impl Grants {
    /// The grants a manifest describes. The manifest was validated when it was
    /// loaded, so an unknown capability here is a daemon bug, not a plugin's.
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

        let mut storage = std::collections::BTreeMap::new();
        for descriptor in &manifest.storage {
            let id = descriptor.validate().map_err(|e| e.refusal())?;
            storage
                .entry(id)
                .and_modify(|write| *write |= descriptor.writable)
                .or_insert(descriptor.writable);
        }
        let mut commands = std::collections::BTreeMap::new();
        for endpoint in &manifest.commands {
            commands.insert(
                omega_proto::CommandAddress {
                    plugin: manifest
                        .name
                        .parse()
                        .map_err(|e: omega_proto::IdentError| Refusal::invalid(e.to_string()))?,
                    command: endpoint
                        .id
                        .parse()
                        .map_err(|e: omega_proto::IdentError| Refusal::invalid(e.to_string()))?,
                },
                endpoint.signature(&manifest.name),
            );
        }
        for dependency in &manifest.command_dependencies {
            commands.insert(
                omega_proto::CommandAddress::try_from(dependency).map_err(|e| e.refusal())?,
                dependency.signature.clone(),
            );
        }
        Ok(Self {
            commands,
            storage,
            capabilities,
            surfaces,
        })
    }

    pub(crate) fn none() -> Self {
        Self {
            commands: Default::default(),
            storage: Default::default(),
            capabilities: Vec::new(),
            surfaces: HashMap::new(),
        }
    }

    pub(crate) fn storage(&self, id: &str, write: bool) -> Result<(), Refusal> {
        let id = id
            .parse::<omega_proto::storage::StorageId>()
            .map_err(|e| e.refusal())?;
        match self.storage.get(&id) {
            Some(writable) if !write || *writable => Ok(()),
            _ => Err(Refusal::denied(format!(
                "storage access not declared for {id}"
            ))),
        }
    }

    pub(crate) fn command_access(
        &self,
        address: &omega_proto::CommandAddress,
    ) -> Result<(), Refusal> {
        if self.commands.contains_key(address) {
            Ok(())
        } else {
            Err(Refusal::denied(format!(
                "command access not declared for {address}"
            )))
        }
    }
    pub(crate) fn command(
        &self,
        address: &omega_proto::CommandAddress,
        signature: &[u8],
    ) -> Result<(), Refusal> {
        match self.commands.get(address) {
            Some(expected) if expected == signature => Ok(()),
            Some(_) => Err(Refusal::precondition(format!(
                "incompatible command {address}"
            ))),
            None => Err(Refusal::denied(format!(
                "command access not declared for {address}"
            ))),
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
