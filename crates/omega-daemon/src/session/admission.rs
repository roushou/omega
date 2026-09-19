//! Authenticate plugin and operator peers.
//! Plugin identity requires peer credentials and a daemon-issued spawn token.
//! Operators must have the daemon owner's UID. Other peers are refused.
//! Capabilities come only from the daemon's stored manifest.

use omega_proto::Manifest;
use omega_proto::PluginName;
use omega_proto::Refusal;

use crate::authorization::{Grants, Role};
use crate::process::Identity;

/// An admitted peer.
#[derive(Debug)]
pub struct Peer {
    id: PeerId,
    grants: Grants,
    manifest: Option<std::sync::Arc<Manifest>>,
    host: Option<(
        omega_proto::host::ProcessId,
        omega_proto::host::HostId,
        Option<omega_proto::host::InvocationId>,
        omega_proto::Values,
    )>,
}

#[derive(Debug)]
enum PeerId {
    /// A plugin the supervisor spawned and vouches for.
    Plugin(PluginName),
    /// The daemon's own user, holding no plugin's grants.
    Operator(i32),
}

impl Peer {
    pub(crate) fn host(
        process: omega_proto::host::ProcessId,
        host: omega_proto::host::HostId,
        invocation: Option<omega_proto::host::InvocationId>,
        manifest: std::sync::Arc<Manifest>,
        settings: omega_proto::Values,
    ) -> Result<Self, Refusal> {
        let name = host
            .as_str()
            .parse()
            .map_err(|error: omega_proto::IdentError| Refusal::invalid(error.to_string()))?;
        Ok(Self {
            grants: Grants::of(&manifest)?,
            manifest: Some(manifest),
            host: Some((process, host, invocation, settings)),
            id: PeerId::Plugin(name),
        })
    }
    pub(crate) fn host_process(&self) -> Option<omega_proto::host::ProcessId> {
        self.host.as_ref().map(|host| host.0)
    }
    pub(crate) fn host_assignment(&self) -> Option<omega_proto::omega::HostAssignment> {
        self.host
            .as_ref()
            .map(|host| omega_proto::omega::HostAssignment {
                process_id: host.0.get(),
                invocation_id: host.2.map_or(0, |id| id.get()),
            })
    }
    pub(crate) fn host_settings(&self) -> Option<omega_proto::Values> {
        self.host.as_ref().map(|host| host.3.clone())
    }
    pub(crate) fn manifest(&self) -> Option<&Manifest> {
        self.manifest.as_deref()
    }

    pub fn plugin(name: PluginName, manifest: &Manifest) -> Result<Self, Refusal> {
        Ok(Self {
            manifest: Some(std::sync::Arc::new(manifest.clone())),
            host: None,
            grants: Grants::of(manifest)?,
            id: PeerId::Plugin(name),
        })
    }

    /// Authenticate a tokenless operator using the daemon owner's peer UID.
    pub fn operator(pid: i32, uid: u32) -> Result<Self, Refusal> {
        if uid != Identity::uid() {
            return Err(Refusal::unauthenticated(
                "not a plugin of this daemon, and not its owner",
            ));
        }

        Ok(Self {
            manifest: None,
            host: None,
            id: PeerId::Operator(pid),
            // An operator holds no plugin's capabilities: it may cycle a plugin,
            // not act as one.
            grants: Grants::none(),
        })
    }

    pub fn role(&self) -> Role {
        match self.id {
            PeerId::Plugin(_) => Role::Plugin,
            PeerId::Operator(_) => Role::Operator,
        }
    }

    /// The plugin this peer is, if it is one.
    pub fn plugin_name(&self) -> Option<&PluginName> {
        match &self.id {
            PeerId::Plugin(name) => Some(name),
            PeerId::Operator(_) => None,
        }
    }

    /// How the peer is named in logs and in `Welcome`.
    pub fn label(&self) -> String {
        match &self.id {
            PeerId::Plugin(name) => name.to_string(),
            PeerId::Operator(pid) => format!("operator-{pid}"),
        }
    }

    pub fn grants(&self) -> &Grants {
        &self.grants
    }
}
