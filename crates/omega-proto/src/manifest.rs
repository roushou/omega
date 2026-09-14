//! Manifest validation and canonical SHA-256 hashing.
//! Repeated fields are sorted and deduplicated before protobuf encoding.

use prost::Message as _;

use crate::Address;
use crate::ident::{IdentError, SurfaceId, UnitName};
use crate::omega::{Capability, EventKind, Manifest, Surface, SurfaceKind};

impl Manifest {
    /// Flag requesting canonical manifest bytes from a compiled plugin.
    pub const DESCRIBE: &'static str = "--omega-manifest";

    /// What a built unit's manifest is called beside its binary.
    pub const FILE_NAME: &'static str = "unit.pb";

    /// Construct an empty manifest.
    pub fn new(name: &UnitName, version: impl Into<String>) -> Self {
        Self {
            name: name.to_string(),
            version: version.into(),
            capabilities: Vec::new(),
            surfaces: Vec::new(),
            commands: Vec::new(),
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

    pub fn serving(
        mut self,
        commands: impl IntoIterator<Item = crate::omega::CommandEndpoint>,
    ) -> Self {
        self.commands = commands.into_iter().collect();
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

    /// Return the manifest with a different unit name.
    pub fn with_name(mut self, name: &UnitName) -> Self {
        self.name = name.to_string();
        self
    }

    /// The unit this manifest describes.
    pub fn unit(&self) -> Result<UnitName, ManifestError> {
        UnitName::parse(self.name.clone()).map_err(ManifestError::from)
    }

    /// Encode deterministic manifest bytes after sorting and deduplicating repeated fields.
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
        canonical.commands.sort_unstable_by(|a, b| a.id.cmp(&b.id));
        canonical.commands.dedup();

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

    /// Decode capabilities, rejecting unknown enum values.
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

    /// Parse declared topic addresses, rejecting invalid names.
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

    /// Validate manifest identity, grants, topics, and surface declarations.
    /// Shared by build-time and daemon load-time validation.
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
        for command in &self.commands {
            SurfaceId::parse(&command.id)?;
        }
        for surface in &self.surfaces {
            surface.surface_id()?;
        }
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

    /// Decode the declared surface kind; reject unknown and unspecified values.
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

/// Invalid manifest identity, capability, topic, or surface declaration.
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
