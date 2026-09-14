//! Driving `cargo` over the config workspace.

use omega_host::{Layout, Profile};
use omega_proto::UnitName;

#[derive(Debug, thiserror::Error)]
pub(crate) enum CargoError {
    #[error("cannot run cargo: {0}")]
    Spawn(#[from] std::io::Error),
    #[error("cargo metadata failed: {0}")]
    Metadata(String),
    #[error("invalid cargo metadata: {0}")]
    Decode(#[from] serde_json::Error),
    #[error("cargo build failed")]
    Failed,
}

/// The cargo invocations omega makes, always against the config workspace and
/// always into omega's own target dir.
#[derive(Debug)]
pub(crate) struct Cargo<'a> {
    layout: &'a Layout,
}

impl<'a> Cargo<'a> {
    pub(crate) fn new(layout: &'a Layout) -> Self {
        Self { layout }
    }

    /// The complete config workspace, including plugins and system.
    pub(crate) async fn build(&self, profile: Profile) -> Result<(), CargoError> {
        self.run(profile, &[]).await
    }

    /// Compile one plugin for the development loop.
    pub(crate) async fn build_unit(
        &self,
        profile: Profile,
        unit: &UnitName,
    ) -> Result<(), CargoError> {
        self.run(profile, &["--package", unit.as_str()]).await
    }

    /// Resolve dependency provenance without fetching or updating the lockfile.
    pub(crate) async fn packages(&self) -> Result<Vec<Package>, CargoError> {
        let output = tokio::process::Command::new("cargo")
            .current_dir(&self.layout.config)
            .args(["metadata", "--format-version=1", "--offline", "--locked"])
            .kill_on_drop(true)
            .output()
            .await?;
        if !output.status.success() {
            return Err(CargoError::Metadata(
                String::from_utf8_lossy(&output.stderr).trim().into(),
            ));
        }
        let metadata: Metadata = serde_json::from_slice(&output.stdout)?;
        Ok(metadata.packages)
    }

    fn build_command(&self, profile: Profile, extra: &[&str]) -> tokio::process::Command {
        let mut command = tokio::process::Command::new("cargo");
        command
            // Cargo resolves local overrides from the working directory, not `--manifest-path`.
            .current_dir(&self.layout.config)
            .arg("build")
            .arg("--manifest-path")
            .arg(self.layout.workspace_manifest())
            .arg("--target-dir")
            .arg(self.layout.target_dir())
            .args(extra);

        if extra.is_empty() {
            command.arg("--workspace");
        }

        // Debug is Cargo's default and requires no profile flag.
        if let Some(flag) = profile.flag() {
            command.arg(flag);
        }

        command
    }

    async fn run(&self, profile: Profile, extra: &[&str]) -> Result<(), CargoError> {
        let status = self.build_command(profile, extra).status().await?;
        status.success().then_some(()).ok_or(CargoError::Failed)
    }
}

#[derive(Debug, serde::Deserialize)]
struct Metadata {
    packages: Vec<Package>,
}

/// A resolved Cargo package; a missing source identifies a local path dependency.
#[derive(Debug, serde::Deserialize)]
pub(crate) struct Package {
    pub(crate) name: String,
    pub(crate) version: String,
    pub(crate) source: Option<String>,
    pub(crate) manifest_path: std::path::PathBuf,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsStr;
    use std::path::Path;

    #[test]
    fn builds_use_config_local_overrides_and_target_directory_for_both_profiles() {
        let layout = Layout::at("/desktop config", "/state", "/cache");
        let cargo = Cargo::new(&layout);
        for (profile, selection, suffix) in [
            (Profile::Debug, vec![], vec!["--workspace"]),
            (Profile::Release, vec![], vec!["--workspace", "--release"]),
            (
                Profile::Debug,
                vec!["--package", "battery"],
                vec!["--package", "battery"],
            ),
            (
                Profile::Release,
                vec!["--package", "battery"],
                vec!["--package", "battery", "--release"],
            ),
        ] {
            let command = cargo.build_command(profile, &selection);
            let command = command.as_std();
            assert_eq!(command.get_program(), OsStr::new("cargo"));
            assert_eq!(
                command.get_current_dir(),
                Some(Path::new("/desktop config"))
            );
            let expected = [
                "build",
                "--manifest-path",
                "/desktop config/Cargo.toml",
                "--target-dir",
                "/desktop config/target",
            ]
            .into_iter()
            .chain(suffix)
            .map(OsStr::new)
            .collect::<Vec<_>>();
            assert_eq!(command.get_args().collect::<Vec<_>>(), expected);
        }
    }
}
