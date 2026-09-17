//! Authenticate plugin and operator peers.
//! Plugin identity requires peer credentials and a daemon-issued spawn token.
//! Operators must have the daemon owner's UID. Other peers are refused.
//! Capabilities come only from the daemon's stored manifest.

use omega_proto::Manifest;
use omega_proto::Refusal;
use omega_proto::UnitName;

use crate::authorization::{Grants, Role};
use crate::process::Identity;

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
