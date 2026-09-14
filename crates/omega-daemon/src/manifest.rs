//! The unit manifests the daemon vouches for, read from the state dir.

use std::collections::HashMap;
use std::path::PathBuf;

use omega_host::Layout;
use omega_host::StateConfig;
use omega_proto::{Manifest, ManifestError, UnitName};

/// A unit's canonical manifest and the hash a peer must present to claim it.
#[derive(Debug, Clone)]
pub struct UnitManifest {
    pub manifest: Manifest,
    pub hash: String,
}

/// Units keyed by name. Immutable after load.
#[derive(Debug, Default)]
pub struct ManifestStore {
    units: HashMap<UnitName, UnitManifest>,
}

impl ManifestStore {
    /// Load and validate built manifests. Hash the exact staged canonical bytes.
    pub fn load(config: &StateConfig, layout: &Layout) -> Result<Self, ManifestStoreError> {
        let mut units = HashMap::with_capacity(config.units.len());

        for name in config.names() {
            let manifest = Self::read(layout, name).map_err(|source| ManifestStoreError {
                unit: name.clone(),
                source,
            })?;

            let hash = manifest.hash();
            units.insert(name.clone(), UnitManifest { manifest, hash });
        }

        Ok(Self { units })
    }

    fn read(layout: &Layout, name: &UnitName) -> Result<Manifest, ManifestReadError> {
        let path = layout.state_unit_manifest(name);
        let bytes = std::fs::read(&path).map_err(|source| ManifestReadError::Unreadable {
            path: path.clone(),
            source,
        })?;

        let manifest = Manifest::decode_bytes(&bytes)?;
        manifest.validate(name)?;
        Ok(manifest)
    }

    /// Construct a manifest store from in-memory values.
    /// Panics if a manifest contains an invalid unit identifier.
    pub fn from_manifests(manifests: impl IntoIterator<Item = Manifest>) -> Self {
        Self {
            units: manifests
                .into_iter()
                .map(|manifest| {
                    let name = manifest
                        .unit()
                        .expect("a manifest built in process has a usable name");
                    let hash = manifest.hash();
                    (name, UnitManifest { manifest, hash })
                })
                .collect(),
        }
    }

    pub fn get(&self, name: &UnitName) -> Option<&UnitManifest> {
        self.units.get(name)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&UnitName, &UnitManifest)> {
        self.units.iter()
    }

    pub fn len(&self) -> usize {
        self.units.len()
    }

    pub fn is_empty(&self) -> bool {
        self.units.is_empty()
    }
}

/// A unit whose manifest could not be loaded or trusted.
#[derive(Debug, thiserror::Error)]
#[error("unit {unit}: {source}")]
pub struct ManifestStoreError {
    pub unit: UnitName,
    #[source]
    pub source: ManifestReadError,
}

/// Why one manifest did not load.
#[derive(Debug, thiserror::Error)]
pub enum ManifestReadError {
    #[error("cannot read {path}: {source}")]
    Unreadable {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error(transparent)]
    Invalid(#[from] ManifestError),
}
