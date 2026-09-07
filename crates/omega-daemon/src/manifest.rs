//! The unit manifests the daemon vouches for, read from the state dir.

use std::collections::HashMap;

use crate::host::StateConfig;
use omega_proto::Manifest;
use omega_proto::{Layout, UnitName};

use crate::error::ManifestStoreError;

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
    /// Load and validate the manifest of every unit in the state config. A
    /// unit whose manifest lies about its identity fails the load rather than
    /// starting unvouched.
    pub fn load(config: &StateConfig, layout: &Layout) -> Result<Self, ManifestStoreError> {
        let mut units = HashMap::with_capacity(config.units.len());

        for name in config.names() {
            let manifest = layout
                .file::<Manifest>(name)
                .read_valid(name)
                .map_err(|source| ManifestStoreError {
                    unit: name.clone(),
                    source,
                })?;

            let hash = manifest.hash();
            units.insert(name.clone(), UnitManifest { manifest, hash });
        }

        Ok(Self { units })
    }

    /// A store built from manifests already in memory (tests, dev).
    pub fn from_manifests(manifests: impl IntoIterator<Item = Manifest>) -> Self {
        Self {
            units: manifests
                .into_iter()
                .map(|manifest| {
                    let hash = manifest.hash();
                    (manifest.name.clone(), UnitManifest { manifest, hash })
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
