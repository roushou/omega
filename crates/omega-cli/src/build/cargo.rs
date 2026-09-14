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

    /// The whole workspace: every unit, and the config plane with them.
    pub(crate) async fn build(&self, profile: Profile) -> Result<(), CargoError> {
        self.run(profile, &[]).await
    }

    /// One unit. The inner loop compiles what changed, not what did not:
    /// `omega dev` waits for this between every save.
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

    async fn run(&self, profile: Profile, extra: &[&str]) -> Result<(), CargoError> {
        let mut command = tokio::process::Command::new("cargo");
        command
            // Run *in* the config, not merely on it. Cargo discovers
            // `.cargo/config.toml` by walking up from the current directory
            // rather than from the manifest, and that file is where a config
            // says which omega it builds against.
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

        // Debug is cargo's default profile and is asked for by asking for
        // nothing, so the flag is an option rather than a string.
        if let Some(flag) = profile.flag() {
            command.arg(flag);
        }

        let status = command.status().await?;
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
