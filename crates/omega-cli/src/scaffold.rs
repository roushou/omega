//! What `omega init` writes, declared as data.
//!
//! The dependency table below is the only place a generated workspace's
//! dependencies are named. A unit crate's `[dependencies]` is *derived* from
//! it — every entry inherited with `workspace = true` — so the two can never
//! drift apart.

use std::io;
use std::path::{Path, PathBuf};

use omega_daemon::host::cargo::{
    CargoManifest, Dependencies, Dependency, DependencySource, DependencySpec, Package, Profile,
    ReleaseProfile, Workspace,
};
use omega_host::Layout;
use omega_host::{Toml, TomlError};
use omega_proto::UnitName;

/// The templates a new config is stamped from, compiled into the binary so a
/// scaffold never depends on omega's source tree being present.
const UNIT_MAIN: &str = include_str!("../templates/unit/src/main.rs");
const SYSTEM_MAIN: &str = include_str!("../templates/system/src/main.rs");

const CRATE_TOKEN: &str = "{unit_snake}";

/// Bundled starting points for a plugin.
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum Template {
    /// A text widget with no system dependencies.
    Minimal,
    /// Battery charge, remaining time, and configurable low-charge styling.
    Battery,
}

impl Template {
    pub fn library(self) -> &'static str {
        match self {
            Self::Minimal => include_str!("../templates/minimal/src/lib.rs"),
            Self::Battery => include_str!("../templates/battery/src/lib.rs"),
        }
    }
}

/// The edition generated crates are written against.
const EDITION: &str = "2024";

#[derive(Debug, thiserror::Error)]
pub enum ScaffoldError {
    #[error("cannot resolve the path to {crate_name}: {source}")]
    CratePath {
        crate_name: &'static str,
        #[source]
        source: io::Error,
    },
    #[error("the bundled program template no longer contains `{token}`")]
    ProgramTemplate { token: &'static str },
    #[error("{} is not an omega checkout: it has no crates/omega", path.display())]
    NotACheckout { path: PathBuf },
    #[error("the checkout declares no workspace version")]
    NoVersion,
    #[error("cannot read the checkout: {0}")]
    Toml(#[from] TomlError),
}

/// Where a generated config gets omega's crates from: always the published
/// ones, because a config is a git repository that must build on every
/// machine it is cloned onto. Building against a checkout is a `[patch]`
/// override instead — see [`SourceTree`].
#[derive(Debug, Clone)]
pub struct Published {
    version: String,
}

impl Published {
    /// The crates that ship with this CLI. They are versioned together, so
    /// the CLI that scaffolded a config is the version it asks for.
    pub fn matching_this_cli() -> Self {
        Self {
            version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }

    pub fn at(version: impl Into<String>) -> Self {
        Self {
            version: version.into(),
        }
    }

    pub fn version(&self) -> &str {
        &self.version
    }
}

/// Generates the files that make `~/.config/omega` a Rust workspace.
#[derive(Debug)]
pub struct Scaffold {
    published: Published,
}

impl Scaffold {
    /// What a plugin crate depends on.
    ///
    /// One crate. A plugin holds handles, draws a view, and returns
    /// `omega::Result` — the protocol, the runtime and the manifest are all
    /// behind that one name, and none of them is a plugin author's problem.
    pub(crate) const UNIT_DEPENDENCIES: &'static [DependencySpec] =
        &[DependencySpec::omega("omega").published_as("omega-rs")];

    /// What the config plane depends on: the document, and nothing else.
    ///
    /// A separate list because the two crates are two audiences. One list for
    /// both made every unit declare the authoring API it never calls, and the
    /// config plane declare an async runtime for a program that computes a
    /// value and exits.
    pub(crate) const SYSTEM_DEPENDENCIES: &'static [DependencySpec] =
        &[DependencySpec::omega("omega-document")];

    pub fn new() -> Self {
        Self::from_source(Published::matching_this_cli())
    }

    /// A scaffold that asks for a chosen version.
    pub fn from_source(published: Published) -> Self {
        Self { published }
    }

    /// The omega crates a generated config depends on, and so the ones a
    /// checkout patches.
    ///
    /// The specs, not their names: a `[patch]` is keyed by what the registry
    /// calls a crate and a dependency table by what the config calls it, and
    /// for the SDK those differ.
    pub fn omega_crates() -> impl Iterator<Item = &'static DependencySpec> {
        Self::UNIT_DEPENDENCIES
            .iter()
            .chain(Self::SYSTEM_DEPENDENCIES)
            .filter(|spec| matches!(spec.source, DependencySource::OmegaCrate))
    }

    /// What git should not carry: the build output, and the file that says
    /// where this machine keeps omega.
    pub fn gitignore(&self) -> &'static str {
        "/target\n/.cargo/\n"
    }

    /// `~/.config/omega/Cargo.toml`: the workspace every unit crate joins.
    pub fn workspace_manifest(&self) -> CargoManifest {
        // Everything either member inherits, declared once at the root. The
        // table is keyed by name, so a dependency both of them want is
        // resolved once and written once.
        let mut dependencies = Dependencies::new();
        for spec in Self::UNIT_DEPENDENCIES
            .iter()
            .chain(Self::SYSTEM_DEPENDENCIES)
        {
            dependencies.insert(spec.name, self.resolve(spec));
        }

        CargoManifest {
            workspace: Some(Workspace {
                resolver: Some("3".into()),
                members: vec![Layout::SYSTEM_CRATE.to_string(), "units/*".into()],
                dependencies,
                ..Default::default()
            }),
            profile: Some(Profile {
                release: Some(ReleaseProfile {
                    strip: Some(true),
                    lto: Some("thin".into()),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    /// `units/<name>/Cargo.toml`, inheriting the workspace's dependencies.
    pub fn unit_crate_manifest(&self, name: &UnitName) -> CargoManifest {
        CargoManifest {
            package: Some(Package::new(name.as_str(), "0.1.0", EDITION)),
            dependencies: Dependencies::from_iter(
                Self::UNIT_DEPENDENCIES
                    .iter()
                    .map(DependencySpec::inherited),
            ),
            ..Default::default()
        }
    }

    /// `units/<name>/src/main.rs`: the program, which is the library and a
    /// call.
    pub fn unit_main(&self, name: &UnitName) -> Result<String, ScaffoldError> {
        Self::stamp(UNIT_MAIN, name)
    }

    /// `system/Cargo.toml`: the configuration plane's crate, a member of the
    /// same workspace as the plugins it configures.
    ///
    /// It is founded depending on no plugin, because founding a config and
    /// writing a plugin are different days. `omega new` adds each plugin as
    /// it is written — see [`Scaffold::depends_on`].
    pub fn system_manifest(&self) -> CargoManifest {
        CargoManifest {
            package: Some(Package::new(Layout::SYSTEM_CRATE, "0.1.0", EDITION)),
            dependencies: Dependencies::from_iter(
                Self::SYSTEM_DEPENDENCIES
                    .iter()
                    .map(DependencySpec::inherited),
            ),
            ..Default::default()
        }
    }

    /// How the config plane depends on one plugin.
    ///
    /// Depending on a plugin is what makes its settings a type rather than a
    /// map of strings — so this is added for every plugin, and it is the one
    /// edit `omega new` makes to a file the author owns, because a path
    /// between two crates in one workspace is bookkeeping and not a decision.
    pub fn depends_on(unit: &UnitName) -> Dependency {
        Dependency::local(format!("../units/{unit}"), &[])
    }

    /// The line `omega new` prints, to paste into the config plane. Fully
    /// qualified so it compiles wherever it lands. Lives beside the plugin
    /// template because every name in it must exist there; a test holds the
    /// two together.
    pub fn placement_hint(unit: &UnitName) -> String {
        let krate = unit.as_str().replace('-', "_");
        format!("omega_document::shell::PluginWidget::new(\"{unit}\", {krate}::UNIT).into()")
    }

    /// `system/src/main.rs`: a document with an empty bar.
    ///
    /// It names no unit, because at the moment a config is founded there are
    /// none. What goes in the bar is the author's to write, and `omega new`
    /// prints the line rather than editing this file: where a widget sits and
    /// how it is configured is the one part of scaffolding that is a decision.
    pub fn system_main(&self) -> &'static str {
        SYSTEM_MAIN
    }

    /// A template with the unit's name in it, or a loud failure.
    ///
    /// A `str::replace` that matches nothing is silent, and the failure it
    /// produces arrives commands later as a config that does not build. This
    /// is where it is caught.
    fn stamp(template: &str, name: &UnitName) -> Result<String, ScaffoldError> {
        let stamped = template.replace(CRATE_TOKEN, &name.as_str().replace('-', "_"));

        // A `str::replace` that matched nothing is silent, and a template
        // that still has a token in it is a file that will not compile.
        if stamped == template || stamped.contains("{unit") {
            Err(ScaffoldError::ProgramTemplate { token: CRATE_TOKEN })
        } else {
            Ok(stamped)
        }
    }

    fn resolve(&self, spec: &DependencySpec) -> Dependency {
        match spec.source {
            DependencySource::Registry(version) => Dependency::registry(version, spec.features),
            DependencySource::OmegaCrate => match spec.package {
                None => Dependency::registry(self.published.version(), spec.features),
                Some(package) => {
                    Dependency::renamed(package, self.published.version(), spec.features)
                }
            },
        }
    }
}

impl Default for Scaffold {
    fn default() -> Self {
        Self::new()
    }
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
    pub fn detect() -> Option<Self> {
        if let Some(named) = std::env::var_os(Self::ENV) {
            return Self::at(named).ok();
        }

        let built_from = Self::built_from();
        if Self::is_cargos_own(&built_from) {
            None
        } else {
            Self::at(built_from).ok()
        }
    }

    /// Whether a path is inside cargo's own storage, and so nobody's to keep.
    fn is_cargos_own(path: &Path) -> bool {
        let home = std::env::var_os("CARGO_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cargo")));

        home.is_some_and(|home| path.starts_with(home))
    }

    /// A checkout at a path, checked for actually being one.
    pub fn at(path: impl Into<PathBuf>) -> Result<Self, ScaffoldError> {
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
            Err(ScaffoldError::NotACheckout { path: root })
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
    pub fn version(&self) -> Result<String, ScaffoldError> {
        // The crates inherit the workspace's version, so the answer is one
        // directory up from any of them.
        let root = self.root().join("Cargo.toml");
        let source = std::fs::read_to_string(&root).map_err(|source| ScaffoldError::CratePath {
            crate_name: "the workspace manifest",
            source,
        })?;

        Toml::decode::<CargoManifest>(&source)?
            .workspace
            .and_then(|workspace| workspace.package)
            .and_then(|package| package.version)
            .ok_or(ScaffoldError::NoVersion)
    }

    /// The patch that points a config's dependencies at this checkout.
    pub fn patch(&self) -> Result<Dependencies, ScaffoldError> {
        let mut patched = Dependencies::new();
        for spec in Scaffold::omega_crates() {
            let path = self.crate_path(spec.name)?;
            patched.insert(spec.package(), Dependency::local(path, &[]));
        }
        Ok(patched)
    }

    /// An absolute, canonical path. Fails loudly rather than emitting a
    /// relative path that would break the moment cargo resolves it.
    fn crate_path(&self, crate_name: &'static str) -> Result<String, ScaffoldError> {
        let path = self.crates_dir.join(crate_name);
        let canonical = path
            .canonicalize()
            .map_err(|source| ScaffoldError::CratePath { crate_name, source })?;
        Ok(canonical.display().to_string())
    }
}
