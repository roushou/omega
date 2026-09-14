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

/// An omega checkout on this machine.
///
/// Only ever a local override. A config's manifest names the published
/// crates; this is what `[patch]` points them at instead, so somebody working
/// on omega's internals builds their desktop against their own tree without
/// that tree's path ever reaching a committed file.
#[derive(Debug, Clone)]
pub struct SourceTree {
    crates_dir: PathBuf,
}

impl SourceTree {
    /// The variable that names a checkout, for whoever's clone is not the one
    /// this binary was built from.
    pub const ENV: &'static str = "OMEGA_SOURCE";

    /// The checkout to build against: `$OMEGA_SOURCE`, else the tree this
    /// binary was compiled from. Never a tree cargo owns — `cargo install
    /// --git` unpacks into `$CARGO_HOME` and may delete it afterwards.
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

    /// Whether a path is inside cargo's own storage, and so nobody's to keep.
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

        // A path that is not an omega checkout would produce a patch that
        // cargo rejects three commands later, naming a file nobody wrote.
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

    /// Where this checkout is, as a person reads it.
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

    /// An absolute, canonical path. Fails loudly rather than emitting a
    /// relative path that would break the moment cargo resolves it.
    fn crate_path(&self, crate_name: &'static str) -> Result<String, LinkError> {
        let path = self.crates_dir.join(crate_name);
        let canonical = path
            .canonicalize()
            .map_err(|source| LinkError::CratePath { crate_name, source })?;
        Ok(canonical.display().to_string())
    }
}
