//! Driving `cargo` over the config workspace.

use omega_core::{Layout, Profile, UnitName};

#[derive(Debug, thiserror::Error)]
pub enum CargoError {
    #[error("cannot run cargo: {0}")]
    Spawn(#[from] std::io::Error),
    #[error("cargo build failed")]
    Failed,
}

/// The cargo invocations omega makes, always against the config workspace and
/// always into omega's own target dir.
#[derive(Debug)]
pub struct Cargo<'a> {
    layout: &'a Layout,
}

impl<'a> Cargo<'a> {
    pub fn new(layout: &'a Layout) -> Self {
        Self { layout }
    }

    /// The whole workspace: every unit, and the config plane with them.
    pub async fn build(&self, profile: Profile) -> Result<(), CargoError> {
        self.run(profile, &[]).await
    }

    /// One unit. The inner loop compiles what changed, not what did not:
    /// `omega dev` waits for this between every save.
    pub async fn build_unit(&self, profile: Profile, unit: &UnitName) -> Result<(), CargoError> {
        self.run(profile, &["--package", unit.as_str()]).await
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

        // Debug is cargo's default profile and is asked for by asking for
        // nothing, so the flag is an option rather than a string.
        if let Some(flag) = profile.flag() {
            command.arg(flag);
        }

        let status = command.status().await?;
        status.success().then_some(()).ok_or(CargoError::Failed)
    }
}
