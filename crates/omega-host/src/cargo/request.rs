use crate::Profile;
use cargo_metadata::PackageId;
use std::path::PathBuf;
use tokio::process::Command;

/// A nonempty Cargo package ID specification, passed literally as one argument.
/// Cargo validates its package-selection syntax and rejects ambiguous matches.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageSpec(String);

impl std::str::FromStr for PackageSpec {
    type Err = PackageSpecError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::try_from(value.to_owned())
    }
}

impl TryFrom<&str> for PackageSpec {
    type Error = PackageSpecError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl TryFrom<String> for PackageSpec {
    type Error = PackageSpecError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value.trim().is_empty() || value.chars().any(char::is_control) {
            return Err(PackageSpecError(value));
        }
        Ok(Self(value))
    }
}

impl PackageSpec {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, thiserror::Error)]
#[error("Cargo package specification must be nonempty and contain no control characters: {0:?}")]
pub struct PackageSpecError(String);

/// Packages to compile. Selection is required rather than inferred from a directory.
#[derive(Debug, Clone)]
pub enum Selection {
    Workspace,
    Package(PackageSpec),
}

/// Network and lockfile policy. Normal resolution may fetch and update Cargo.lock.
#[derive(Debug, Clone, Copy, Default)]
pub enum Resolution {
    #[default]
    Normal,
    /// Refuse lockfile changes; network access remains permitted.
    Locked,
    /// Disable network access; Cargo may still update the lockfile.
    Offline,
    /// Disable network access and refuse lockfile changes.
    OfflineLocked,
}

impl Resolution {
    pub(super) fn apply(self, command: &mut Command) {
        match self {
            Self::Normal => {}
            Self::Locked => {
                command.arg("--locked");
            }
            Self::Offline => {
                command.arg("--offline");
            }
            Self::OfflineLocked => {
                command.args(["--offline", "--locked"]);
            }
        }
    }
}

/// Compile selected packages. Defaults to the dev profile and normal dependency
/// resolution. Without a target-directory override, Cargo's configuration applies.
#[derive(Debug, Clone)]
pub struct BuildRequest {
    selection: Selection,
    profile: Profile,
    target_dir: Option<PathBuf>,
    resolution: Resolution,
}

impl BuildRequest {
    pub fn new(selection: Selection) -> Self {
        Self {
            selection,
            profile: Profile::Debug,
            target_dir: None,
            resolution: Resolution::Normal,
        }
    }

    pub fn profile(mut self, profile: Profile) -> Self {
        self.profile = profile;
        self
    }

    /// Relative paths are interpreted from the Cargo working directory.
    pub fn target_dir(mut self, directory: impl Into<PathBuf>) -> Self {
        self.target_dir = Some(directory.into());
        self
    }

    pub fn resolution(mut self, resolution: Resolution) -> Self {
        self.resolution = resolution;
        self
    }

    pub(super) fn apply(&self, command: &mut Command) {
        match &self.selection {
            Selection::Workspace => {
                command.arg("--workspace");
            }
            Selection::Package(package) => {
                command.arg(format!("--package={}", package.as_str()));
            }
        }
        if let Some(flag) = self.profile.flag() {
            command.arg(flag);
        }
        if let Some(directory) = &self.target_dir {
            command.arg("--target-dir").arg(directory);
        }
        self.resolution.apply(command);
    }
}

/// Read the complete dependency graph using metadata format version 1.
/// Defaults to normal resolution, which may fetch and update Cargo.lock.
#[derive(Debug, Clone, Copy, Default)]
pub struct MetadataRequest {
    pub(super) resolution: Resolution,
}

impl MetadataRequest {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn resolution(mut self, resolution: Resolution) -> Self {
        self.resolution = resolution;
        self
    }
}

/// Compile, but never run, one package's library tests. The package ID comes from
/// metadata and also scopes executable selection. Defaults to Cargo's test profile,
/// configured target directory, and normal resolution.
///
/// ```no_run
/// use omega_host::cargo::{Cargo, MetadataRequest, TestBuildRequest};
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let cargo = Cargo::new("/home/me/desktop");
/// let metadata = cargo.metadata(MetadataRequest::new()).await?;
/// let package = metadata.workspace_packages().into_iter()
///     .find(|package| package.name.as_str() == "clock")
///     .ok_or("workspace has no clock package")?;
/// let artifacts = cargo.compile_tests(TestBuildRequest::library(package.id.clone())).await?;
/// let executable = artifacts.library_test()?;
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone)]
pub struct TestBuildRequest {
    pub(super) package: PackageId,
    build: BuildRequest,
}

impl TestBuildRequest {
    pub fn library(package: PackageId) -> Self {
        Self {
            build: BuildRequest::new(Selection::Package(PackageSpec(package.to_string()))),
            package,
        }
    }

    /// Debug selects Cargo's default test profile; Release selects its release profile.
    pub fn profile(mut self, profile: Profile) -> Self {
        self.build = self.build.profile(profile);
        self
    }

    pub fn target_dir(mut self, directory: impl Into<PathBuf>) -> Self {
        self.build = self.build.target_dir(directory);
        self
    }

    pub fn resolution(mut self, resolution: Resolution) -> Self {
        self.build = self.build.resolution(resolution);
        self
    }

    pub(super) fn apply(&self, command: &mut Command) {
        self.build.apply(command);
        command.args(["--lib", "--no-run", "--message-format=json"]);
    }
}
