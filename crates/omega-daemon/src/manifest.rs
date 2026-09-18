//! The plugin manifests the daemon vouches for, read from the state dir.

use std::collections::HashMap;
use std::path::PathBuf;

use omega_host::Layout;
use omega_host::StateConfig;
use omega_proto::{Manifest, ManifestError, PluginName};

/// A plugin's canonical manifest and the hash a peer must present to claim it.
#[derive(Debug, Clone)]
pub struct PluginManifest {
    pub manifest: Manifest,
    pub hash: String,
}

/// Plugins keyed by name. Immutable after load.
#[derive(Debug, Default)]
pub struct ManifestStore {
    plugins: HashMap<PluginName, PluginManifest>,
}

impl ManifestStore {
    /// Load and validate built manifests. Hash the exact staged canonical bytes.
    pub fn load(config: &StateConfig, layout: &Layout) -> Result<Self, ManifestStoreError> {
        let mut plugins = HashMap::with_capacity(config.plugins.len());

        for name in config.names() {
            let manifest =
                Self::read(layout, name).map_err(|source| ManifestStoreError::Plugin {
                    plugin: name.clone(),
                    source,
                })?;

            let hash = manifest.hash();
            plugins.insert(name.clone(), PluginManifest { manifest, hash });
        }

        omega_proto::CommandContracts::validate(plugins.values().map(|entry| &entry.manifest))?;
        Ok(Self { plugins })
    }

    fn read(layout: &Layout, name: &PluginName) -> Result<Manifest, ManifestReadError> {
        let path = layout.state_plugin_manifest(name);
        let bytes = std::fs::read(&path).map_err(|source| ManifestReadError::Unreadable {
            path: path.clone(),
            source,
        })?;

        let manifest = Manifest::decode_bytes(&bytes)?;
        manifest.validate(name)?;
        Ok(manifest)
    }

    /// Construct a manifest store from in-memory values.
    /// Panics if a manifest contains an invalid plugin identifier.
    pub fn from_manifests(manifests: impl IntoIterator<Item = Manifest>) -> Self {
        Self {
            plugins: manifests
                .into_iter()
                .map(|manifest| {
                    let name = manifest
                        .plugin()
                        .expect("a manifest built in process has a usable name");
                    let hash = manifest.hash();
                    (name, PluginManifest { manifest, hash })
                })
                .collect(),
        }
    }

    pub fn get(&self, name: &PluginName) -> Option<&PluginManifest> {
        self.plugins.get(name)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&PluginName, &PluginManifest)> {
        self.plugins.iter()
    }

    pub fn len(&self) -> usize {
        self.plugins.len()
    }

    pub fn is_empty(&self) -> bool {
        self.plugins.is_empty()
    }
}

/// A manifest or dependency set that could not be loaded or trusted.
#[derive(Debug, thiserror::Error)]
pub enum ManifestStoreError {
    #[error("plugin {plugin}: {source}")]
    Plugin {
        plugin: PluginName,
        #[source]
        source: ManifestReadError,
    },
    #[error(transparent)]
    Commands(#[from] omega_proto::CommandContractError),
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
