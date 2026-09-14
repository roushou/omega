use anyhow::{Context, bail};
use omega_host::fs::{Changes, Recursion};
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub(super) struct Build {
    pub directory: PathBuf,
    pub package: String,
}
#[derive(Deserialize)]
struct Metadata {
    packages: Vec<Package>,
    workspace_root: PathBuf,
}
#[derive(Deserialize)]
struct Package {
    name: String,
    source: Option<String>,
    manifest_path: PathBuf,
}
impl Build {
    pub(super) async fn watch(&self) -> anyhow::Result<Changes> {
        let result = tokio::process::Command::new("cargo")
            .current_dir(&self.directory)
            .args(["metadata", "--format-version=1"])
            .kill_on_drop(true)
            .output()
            .await?;
        if !result.status.success() {
            bail!(
                "cargo metadata failed: {}",
                String::from_utf8_lossy(&result.stderr)
            );
        }
        let metadata: Metadata = serde_json::from_slice(&result.stdout)?;
        anyhow::ensure!(
            metadata.packages.iter().any(|p| p.name == self.package),
            "unknown preview package {}",
            self.package
        );
        let mut paths = vec![metadata.workspace_root];
        for package in metadata.packages {
            if package.source.is_none() {
                let parent = package
                    .manifest_path
                    .parent()
                    .context("package manifest has no parent")?
                    .to_path_buf();
                if !paths.iter().any(|root| parent.starts_with(root)) {
                    paths.push(parent);
                }
            }
        }
        Ok(Changes::watch(
            &paths.iter().map(|p| p.as_path()).collect::<Vec<_>>(),
            Recursion::Recursive,
        )?)
    }
    fn compile_command(&self) -> tokio::process::Command {
        let mut command = tokio::process::Command::new("cargo");
        command
            .current_dir(&self.directory)
            .args([
                "test",
                "--package",
                &self.package,
                "--lib",
                "--no-run",
                "--message-format=json",
            ])
            .stderr(std::process::Stdio::inherit())
            .kill_on_drop(true);
        command
    }

    pub(super) async fn compile(&self) -> anyhow::Result<PathBuf> {
        let output = self.compile_command().output().await?;
        let mut executable = None;
        let mut diagnostics = String::new();
        for line in output
            .stdout
            .split(|b| *b == b'\n')
            .filter(|l| !l.is_empty())
        {
            let message: serde_json::Value = serde_json::from_slice(line)?;
            if message["reason"] == "compiler-message"
                && let Some(rendered) = message["message"]["rendered"].as_str()
            {
                diagnostics.extend(
                    rendered
                        .chars()
                        .take(65536usize.saturating_sub(diagnostics.len())),
                );
            }
            if message["reason"] == "compiler-artifact"
                && message["profile"]["test"] == true
                && message["target"]["kind"]
                    .as_array()
                    .is_some_and(|k| k.iter().any(|v| v == "lib"))
                && let Some(path) = message["executable"].as_str()
            {
                executable = Some(PathBuf::from(path));
            }
        }
        if !output.status.success() {
            bail!("preview build failed; save to retry\n{diagnostics}");
        }
        executable.context(
            "package has no library test target; register previews::preview in its library",
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsStr;
    use std::path::Path;

    #[test]
    fn preview_compilation_preserves_the_authors_workspace_and_cargo_configuration() {
        let build = Build {
            directory: "/desktop config".into(),
            package: "application-launcher".into(),
        };
        let command = build.compile_command();
        let command = command.as_std();
        assert_eq!(command.get_program(), OsStr::new("cargo"));
        assert_eq!(
            command.get_current_dir(),
            Some(Path::new("/desktop config"))
        );
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            [
                "test",
                "--package",
                "application-launcher",
                "--lib",
                "--no-run",
                "--message-format=json"
            ]
            .map(OsStr::new)
        );
        assert!(
            command
                .get_envs()
                .all(|(name, _)| name != "CARGO_TARGET_DIR")
        );
    }
}
