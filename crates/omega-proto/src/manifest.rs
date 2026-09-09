//! What a unit asks for, and the hash it proves itself with.
//!
//! The manifest is a schema message like everything else that crosses the
//! socket. It used to be TOML, hashed over a canonical string that only
//! `toml_edit` could produce — which meant the one thing a peer must compute
//! byte-for-byte to connect was the one thing no other language could
//! compute without reimplementing a Rust crate.
//!
//! Now the hash is sha256 over [`Manifest::canonical`]: every repeated field
//! sorted and deduplicated, then encoded in tag order. Any protobuf
//! implementation reaches the same bytes.

use prost::Message as _;

use crate::Address;
use crate::ident::{IdentError, SurfaceId, UnitName};
use crate::omega::{Capability, EventKind, Manifest, Surface, SurfaceKind};

impl Manifest {
    /// The flag that makes a compiled plugin print its manifest and exit.
    ///
    /// A plugin's manifest is not a file anyone writes — it is what the
    /// plugin's own fields add up to, and this is how the build asks. The
    /// config plane is asked the same way what the machine should be.
    pub const DESCRIBE: &'static str = "--omega-manifest";

    /// What a built unit's manifest is called beside its binary.
    pub const FILE_NAME: &'static str = "unit.pb";

    /// A manifest with nothing declared, which is what a plugin holding no
    /// fields adds up to.
    pub fn new(name: &UnitName, version: impl Into<String>) -> Self {
        Self {
            name: name.to_string(),
            version: version.into(),
            capabilities: Vec::new(),
            surfaces: Vec::new(),
            state_topics: Vec::new(),
            events: Vec::new(),
        }
    }

    pub fn granting(mut self, capabilities: impl IntoIterator<Item = Capability>) -> Self {
        self.capabilities = capabilities.into_iter().map(|c| c as i32).collect();
        self
    }

    pub fn exposing(mut self, surfaces: impl IntoIterator<Item = Surface>) -> Self {
        self.surfaces = surfaces.into_iter().collect();
        self
    }

    pub fn reading(mut self, topics: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.state_topics = topics.into_iter().map(Into::into).collect();
        self
    }

    pub fn handling(mut self, events: impl IntoIterator<Item = EventKind>) -> Self {
        self.events = events.into_iter().map(|e| e as i32).collect();
        self
    }

    /// The same manifest, renamed — how a template becomes a unit's manifest.
    pub fn with_name(mut self, name: &UnitName) -> Self {
        self.name = name.to_string();
        self
    }

    /// The unit this manifest describes.
    pub fn unit(&self) -> Result<UnitName, ManifestError> {
        UnitName::parse(self.name.clone()).map_err(ManifestError::from)
    }

    /// Deterministic bytes: sorted, deduplicated, encoded in tag order.
    ///
    /// Sorting is what makes the hash a property of what a unit declares
    /// rather than of the order its fields happened to be visited in. A
    /// plugin that lists two capabilities the other way round is the same
    /// plugin, and must present the same hash.
    pub fn canonical(&self) -> Vec<u8> {
        let mut canonical = self.clone();

        canonical.capabilities.sort_unstable();
        canonical.capabilities.dedup();
        canonical.events.sort_unstable();
        canonical.events.dedup();
        canonical.state_topics.sort_unstable();
        canonical.state_topics.dedup();
        canonical
            .surfaces
            .sort_unstable_by(|a, b| (&a.id, a.kind).cmp(&(&b.id, b.kind)));
        canonical.surfaces.dedup();

        canonical.encode_to_vec()
    }

    /// sha256 of the canonical encoding, as lowercase hex.
    pub fn hash(&self) -> String {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(self.canonical());
        hex::encode(hasher.finalize())
    }

    /// Decode a manifest from the bytes a plugin printed or a build staged.
    pub fn decode_bytes(bytes: &[u8]) -> Result<Self, ManifestError> {
        Self::decode(bytes).map_err(ManifestError::Malformed)
    }

    /// Capabilities as the wire enum values the daemon grants.
    ///
    /// Fails loud on a value this build does not know: a capability that
    /// cannot be named is a grant that cannot be reasoned about, and
    /// dropping it silently is how a unit ends up running with less than it
    /// declared and failing somewhere else entirely.
    pub fn granted(&self) -> Result<Vec<Capability>, ManifestError> {
        self.capabilities
            .iter()
            .map(|value| match Capability::try_from(*value) {
                Ok(Capability::Unspecified) | Err(_) => {
                    Err(ManifestError::UnknownCapability(*value))
                }
                Ok(capability) => Ok(capability),
            })
            .collect()
    }

    /// Declared surfaces as wire enum values, in declaration order.
    pub fn surface_kinds(&self) -> Result<Vec<SurfaceKind>, ManifestError> {
        self.surfaces.iter().map(Surface::declared).collect()
    }

    /// The state topics this unit declares it reads, as validated addresses.
    /// A typo is a build error, not a subscription that silently matches
    /// nothing.
    pub fn addresses(&self) -> Result<Vec<Address>, ManifestError> {
        self.state_topics
            .iter()
            .map(|address| Address::parse(address).map_err(ManifestError::from))
            .collect()
    }

    /// The events this unit declares it handles.
    pub fn event_kinds(&self) -> Result<Vec<EventKind>, ManifestError> {
        self.events
            .iter()
            .map(|value| match EventKind::try_from(*value) {
                Ok(EventKind::Unspecified) | Err(_) => Err(ManifestError::UnknownEvent(*value)),
                Ok(kind) => Ok(kind),
            })
            .collect()
    }

    /// Identity, grants, and every string that addresses something on the
    /// wire — checked in one place: the CLI checks it at build time and the
    /// daemon checks it again at load time, and neither owns a private copy
    /// of the rule.
    pub fn validate(&self, unit: &UnitName) -> Result<(), ManifestError> {
        let declared = self.unit()?;
        if &declared != unit {
            return Err(ManifestError::NameMismatch {
                expected: unit.clone(),
                declared,
            });
        }
        self.granted()?;
        self.surface_kinds()?;
        self.addresses()?;
        self.event_kinds()?;
        Ok(())
    }
}

impl Surface {
    pub fn new(id: &SurfaceId, kind: SurfaceKind) -> Self {
        Self {
            id: id.to_string(),
            kind: kind as i32,
        }
    }

    /// The declared kind, as the canonical wire enum.
    ///
    /// Errors on a kind this build does not know *and* on the unspecified
    /// zero, which is what an unset field decodes to — prost's own `kind()`
    /// maps both to `Unspecified`, and a surface whose kind cannot be named
    /// must fail the manifest rather than be quietly dropped.
    pub fn declared(&self) -> Result<SurfaceKind, ManifestError> {
        match SurfaceKind::try_from(self.kind) {
            Ok(SurfaceKind::Unspecified) | Err(_) => {
                Err(ManifestError::UnknownSurfaceKind(self.kind))
            }
            Ok(kind) => Ok(kind),
        }
    }

    /// The surface's id, validated.
    pub fn surface_id(&self) -> Result<SurfaceId, ManifestError> {
        SurfaceId::parse(self.id.clone()).map_err(ManifestError::from)
    }
}

/// A manifest that decoded but does not describe a unit the daemon can trust.
/// Fail loud: an unknown capability or surface kind must be a build error,
/// never a silently dropped grant.
#[derive(Debug, thiserror::Error)]
pub enum ManifestError {
    #[error("not a manifest: {0}")]
    Malformed(#[from] prost::DecodeError),
    #[error("unknown capability {0}")]
    UnknownCapability(i32),
    #[error("unknown surface kind {0}")]
    UnknownSurfaceKind(i32),
    #[error("unknown event {0}")]
    UnknownEvent(i32),
    #[error("{0}")]
    UnknownStateTopic(#[from] crate::AddressError),
    #[error("{0}")]
    Name(#[from] IdentError),
    #[error("unit {expected}: manifest declares name {declared:?} — they must match")]
    NameMismatch {
        expected: UnitName,
        declared: UnitName,
    },
}
