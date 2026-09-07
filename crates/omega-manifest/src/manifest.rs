use omega_core::{Layout, SurfaceId, Toml, TomlError, TomlFile, TomlSchema, UnitName, Validated};
use omega_wire::Topic;
use omega_wire::omega::{Capability, EventKind, SurfaceKind};
use serde::{Deserialize, Serialize};

use crate::error::ManifestError;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Manifest {
    pub name: UnitName,
    pub version: String,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub surfaces: Vec<Surface>,
    #[serde(default)]
    pub state_topics: Vec<String>,
    #[serde(default)]
    pub events: Vec<String>,
}

impl Manifest {
    /// The flag that makes a compiled plugin print its manifest and exit.
    ///
    /// A plugin's manifest is not a file anyone writes — it is what the
    /// plugin's own fields add up to, and this is how the build asks. The
    /// config plane is asked the same way what the machine should be.
    pub const DESCRIBE: &'static str = "--omega-manifest";

    /// A minimal manifest for tests and dev.
    pub fn new(name: UnitName, version: impl Into<String>) -> Self {
        Self {
            name,
            version: version.into(),
            capabilities: Vec::new(),
            surfaces: Vec::new(),
            state_topics: Vec::new(),
            events: Vec::new(),
        }
    }

    /// Parse a manifest from TOML source — what a unit does with its own
    /// `include_str!`'d manifest at startup.
    pub fn parse(src: &str) -> Result<Self, TomlError> {
        Toml::decode(src)
    }

    /// Deterministic serialization: the exact bytes shipped as `unit.toml`,
    /// and the exact bytes hashed.
    pub fn canonical_toml(&self) -> String {
        Toml::encode(self).expect("a manifest is plain data; encoding cannot fail")
    }

    /// sha256 of the canonical manifest.
    pub fn hash(&self) -> String {
        Self::sha256_hex(self.canonical_toml().as_bytes())
    }

    /// Capabilities as the wire enum values the daemon grants. Errors on
    /// unknown capabilities instead of silently omitting them.
    pub fn capabilities(&self) -> Result<Vec<Capability>, ManifestError> {
        self.capabilities
            .iter()
            .map(|name| {
                Capability::from_str_name(name)
                    .ok_or_else(|| ManifestError::UnknownCapability(name.clone()))
            })
            .collect()
    }

    /// Declared surfaces as wire enum values, in declaration order.
    pub fn surface_kinds(&self) -> Result<Vec<SurfaceKind>, ManifestError> {
        self.surfaces.iter().map(Surface::kind).collect()
    }

    /// The state topics this unit declares it reads, as validated addresses.
    /// A typo is a build error, not a subscription that silently matches
    /// nothing.
    pub fn state_topics(&self) -> Result<Vec<Topic>, ManifestError> {
        self.state_topics
            .iter()
            .map(|address| Topic::parse(address).map_err(ManifestError::from))
            .collect()
    }

    /// The events this unit declares it handles, as wire enum values.
    pub fn events(&self) -> Result<Vec<EventKind>, ManifestError> {
        self.events
            .iter()
            .map(|name| {
                EventKind::from_str_name(name)
                    .ok_or_else(|| ManifestError::UnknownEvent(name.clone()))
            })
            .collect()
    }

    /// The same manifest, renamed — how a template becomes a unit's manifest.
    pub fn with_name(mut self, name: UnitName) -> Self {
        self.name = name;
        self
    }

    fn sha256_hex(bytes: &[u8]) -> String {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(bytes);
        hex::encode(hasher.finalize())
    }
}

impl TomlSchema for Manifest {
    const KIND: &'static str = "unit manifest";
    /// The unit whose manifest this is. There is one copy — the one the build
    /// wrote after asking the plugin — so there is nothing else to say.
    type Key<'a> = &'a UnitName;

    fn locate(layout: &Layout, name: Self::Key<'_>) -> TomlFile<Self> {
        TomlFile::at(layout.state_unit_manifest(name))
    }
}

impl Validated for Manifest {
    /// The unit the manifest is supposed to describe.
    type Context<'a> = &'a UnitName;
    type Invalid = ManifestError;

    /// Identity, grants, and every string that addresses something on the
    /// wire — checked in one place: the CLI checks it at build time and the
    /// daemon checks it again at load time, and neither owns a private copy
    /// of the rule.
    fn validate(&self, unit: Self::Context<'_>) -> Result<(), ManifestError> {
        if &self.name != unit {
            return Err(ManifestError::NameMismatch {
                expected: unit.clone(),
                declared: self.name.clone(),
            });
        }
        self.capabilities()?;
        self.surface_kinds()?;
        self.state_topics()?;
        self.events()?;
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Surface {
    pub id: SurfaceId,
    pub kind: String,
}

impl Surface {
    pub fn new(id: SurfaceId, kind: SurfaceKind) -> Self {
        Self {
            id,
            kind: kind.as_str_name().to_string(),
        }
    }

    /// The declared kind, as the canonical wire enum. Errors on unknown kinds
    /// so a typo fails the build rather than silently dropping the surface.
    pub fn kind(&self) -> Result<SurfaceKind, ManifestError> {
        SurfaceKind::from_str_name(&self.kind)
            .ok_or_else(|| ManifestError::UnknownSurfaceKind(self.kind.clone()))
    }
}
