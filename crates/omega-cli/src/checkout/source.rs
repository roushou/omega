use crate::scaffold::Scaffold;
use omega_host::workspace::cargo::{CargoManifest, Dependencies, Dependency};
use omega_host::{Toml, TomlError};
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum LinkError {
    #[error("cannot resolve {crate_name}: {source}")]
    CratePath {
        crate_name: &'static str,
        #[source]
        source: io::Error,
    },
    #[error("{} is not an omega checkout", path.display())]
    NotACheckout { path: PathBuf },
    #[error("the checkout declares no workspace version")]
    NoVersion,
    #[error(transparent)]
    Toml(#[from] TomlError),
}

/// A local Omega checkout used for machine-local Cargo patches.
/// Published dependency requirements remain in the config manifest.
#[derive(Debug, Clone)]
pub struct SourceTree {
    crates_dir: PathBuf,
}

impl SourceTree {
    /// The variable that names a checkout, for whoever's clone is not the one
    /// this binary was built from.
    pub const ENV: &'static str = "OMEGA_SOURCE";

    /// Resolve `$OMEGA_SOURCE`, falling back to the compile-time source tree.
    /// Reject Cargo-managed directories, which may be deleted after installation.
    pub fn detect() -> Result<Option<Self>, LinkError> {
        if let Some(named) = std::env::var_os(Self::ENV) {
            return Self::at(named).map(Some);
        }

        let built_from = Self::built_from();
        Ok(if Self::is_cargos_own(&built_from) {
            None
        } else {
            Self::at(built_from).ok()
        })
    }

    /// Whether the path is inside Cargo-managed storage.
    fn is_cargos_own(path: &Path) -> bool {
        let home = std::env::var_os("CARGO_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cargo")));

        home.is_some_and(|home| path.starts_with(home))
    }

    /// A checkout at a path, checked for actually being one.
    pub fn at(path: impl Into<PathBuf>) -> Result<Self, LinkError> {
        let root = path.into();
        let crates_dir = if root.ends_with("crates") {
            root.clone()
        } else {
            root.join("crates")
        };

        // Validate checkout contents before constructing patch paths.
        if crates_dir.join("omega").join("Cargo.toml").exists() {
            Ok(Self { crates_dir })
        } else {
            Err(LinkError::NotACheckout { path: root })
        }
    }

    /// The tree this binary was compiled from.
    fn built_from() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf()
    }

    /// Display the checkout path.
    pub fn root(&self) -> &Path {
        self.crates_dir.parent().unwrap_or(&self.crates_dir)
    }

    /// The version the crates in this checkout carry, so a config that is
    /// linked to it asks for a version the patch can satisfy.
    pub fn version(&self) -> Result<String, LinkError> {
        // The crates inherit the workspace's version, so the answer is one
        // directory up from any of them.
        let root = self.root().join("Cargo.toml");
        let source = std::fs::read_to_string(&root).map_err(|source| LinkError::CratePath {
            crate_name: "the workspace manifest",
            source,
        })?;

        Toml::decode::<CargoManifest>(&source)?
            .workspace
            .and_then(|workspace| workspace.package)
            .and_then(|package| package.version)
            .ok_or(LinkError::NoVersion)
    }

    /// The patch that points a config's dependencies at this checkout.
    pub fn patch(&self, preview: bool) -> Result<Dependencies, LinkError> {
        let mut patched = Dependencies::new();
        for spec in Scaffold::omega_crates()
            .chain(Scaffold::PREVIEW_DEPENDENCIES.iter().filter(|_| preview))
        {
            let path = self.crate_path(spec.name)?;
            patched.insert(spec.package(), Dependency::local(path, &[]));
        }
        Ok(patched)
    }

    /// Resolve an absolute canonical path or return an error.
    fn crate_path(&self, crate_name: &'static str) -> Result<String, LinkError> {
        let path = self.crates_dir.join(crate_name);
        let canonical = path
            .canonicalize()
            .map_err(|source| LinkError::CratePath { crate_name, source })?;
        Ok(canonical.display().to_string())
    }
}
