//! Workspace templates and dependency specifications used by scaffolding.
//! Generated member dependencies inherit the shared declarations.

use omega_host::Layout;
use omega_host::cargo::{CargoError, Dependencies, Dependency, Inherited, Manifest};
use omega_proto::UnitName;

mod dependency;
pub use dependency::{DependencySource, DependencySpec};

mod name;
pub use name::PluginName;

/// The templates a new config is stamped from, compiled into the binary so a
/// scaffold never depends on omega's source tree being present.
const UNIT_MAIN: &str = include_str!("../../templates/unit/src/main.rs");
const SYSTEM_MAIN: &str = include_str!("../../templates/system/src/main.rs");

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
            Self::Minimal => include_str!("../../templates/minimal/src/lib.rs"),
            Self::Battery => include_str!("../../templates/battery/src/lib.rs"),
        }
    }
}

/// The edition generated crates are written against.
const EDITION: &str = "2024";

#[derive(Debug, thiserror::Error)]
pub enum ScaffoldError {
    #[error("the bundled program template no longer contains `{token}`")]
    ProgramTemplate { token: &'static str },
}

/// Published dependency specifications. Local checkout paths belong in Cargo patches.
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
    /// Dependencies inherited by generated plugin crates.
    pub(crate) const UNIT_DEPENDENCIES: &'static [DependencySpec] =
        &[DependencySpec::omega("omega").published_as("omega-rs")];

    /// Dependencies inherited by the generated system crate.
    pub(crate) const SYSTEM_DEPENDENCIES: &'static [DependencySpec] = &[
        DependencySpec::omega("omega-document"),
        DependencySpec::omega("omega-omarchy"),
    ];

    /// Optional development tools; enabled only by an explicit workspace dependency.
    pub(crate) const PREVIEW_DEPENDENCIES: &'static [DependencySpec] =
        &[DependencySpec::omega("omega-preview")];

    pub fn new() -> Self {
        Self::from_source(Published::matching_this_cli())
    }

    /// A scaffold that asks for a chosen version.
    pub fn from_source(published: Published) -> Self {
        Self { published }
    }

    /// Dependency specifications patched when linking a checkout.
    /// Registry package names may differ from manifest aliases.
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
    pub fn workspace_manifest(&self) -> Result<Manifest, CargoError> {
        let dependencies = self.workspace_dependencies();

        let mut manifest =
            Manifest::new_workspace("3", EDITION, &[Layout::SYSTEM_CRATE], &dependencies)?;
        manifest.set_release_profile(true, "thin")?;
        Ok(manifest)
    }

    /// Fill absent workspace defaults without overwriting existing user choices.
    pub(crate) fn complete_workspace(&self, manifest: &mut Manifest) -> Result<(), CargoError> {
        manifest.ensure_workspace_defaults(EDITION, &self.workspace_dependencies())
    }

    fn workspace_dependencies(&self) -> Dependencies {
        // Deduplicate inherited dependencies by manifest alias.
        let mut dependencies = Dependencies::new();
        for spec in Self::UNIT_DEPENDENCIES
            .iter()
            .chain(Self::SYSTEM_DEPENDENCIES)
        {
            dependencies.insert(spec.name, self.resolve(spec));
        }

        dependencies
    }

    /// `plugins/<name>/Cargo.toml`, inheriting the workspace's dependencies.
    pub fn unit_crate_manifest(&self, name: &UnitName) -> Result<Manifest, CargoError> {
        Manifest::new_package(
            name.as_str(),
            "0.1.0",
            Inherited::Workspace,
            &Dependencies::from_iter(
                Self::UNIT_DEPENDENCIES
                    .iter()
                    .map(DependencySpec::inherited),
            ),
        )
    }

    /// `plugins/<name>/src/main.rs`: the program, which is the library and a
    /// call.
    pub fn unit_main(&self, name: &PluginName) -> Result<String, ScaffoldError> {
        Self::stamp(UNIT_MAIN, name)
    }

    /// Generate the system crate manifest. Plugin dependencies are added by `omega new`.
    pub fn system_manifest(&self) -> Result<Manifest, CargoError> {
        Manifest::new_package(
            Layout::SYSTEM_CRATE,
            "0.1.0",
            Inherited::Workspace,
            &Dependencies::from_iter(
                Self::SYSTEM_DEPENDENCIES
                    .iter()
                    .map(DependencySpec::inherited),
            ),
        )
    }

    /// Generate the system crate's path dependency on a plugin.
    pub fn depends_on(unit: &UnitName) -> Dependency {
        Dependency::local(format!("../plugins/{unit}"), &[])
    }

    /// Fully qualified placement expression for the generated plugin.
    /// Template tests verify referenced types and names.
    pub fn placement_hint(unit: &PluginName, template: Template) -> String {
        let krate = unit.rust_ident();
        let unit = unit.package();
        let widget = match template {
            Template::Minimal => "Hello",
            Template::Battery => "BatteryWidget",
        };
        format!("omega_omarchy::shell::PluginWidget::new(\"{unit}\", {krate}::{widget}).into()")
    }

    /// Generate a system entry point with an empty bar.
    pub fn system_main(&self) -> &'static str {
        SYSTEM_MAIN
    }

    /// Entry point for a configuration initialized from an imported shell.
    pub fn imported_system_main(&self) -> &'static str {
        include_str!("../../templates/system-import/src/main.rs")
    }

    /// Expand required template placeholders or fail if any are missing.
    fn stamp(template: &str, name: &PluginName) -> Result<String, ScaffoldError> {
        let stamped = template.replace(CRATE_TOKEN, name.rust_ident());

        // Reject unmatched placeholders before writing an invalid template.
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
