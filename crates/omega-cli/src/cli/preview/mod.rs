mod build;
mod capture;
mod runner;
use crate::ui::{Step, Ui};
use anyhow::Context;
use build::Build;
use omega_host::cargo::Cargo;
use omega_host::{AtomicFile, Layout};
use omega_proto::{
    omega::{PreviewRequest, PreviewSnapshot, preview_request},
    preview::{CaseId, Reader, Writer},
};
use runner::Runner;
use std::{os::unix::fs::PermissionsExt, path::PathBuf, time::Duration};

/// Inspect isolated component and surface cases, with rebuilds on source changes.
#[derive(Debug, clap::Args)]
pub struct PreviewCmd {
    pub package: String,
    /// Workspace/package manifest; defaults to the Omega config workspace.
    #[arg(long)]
    pub manifest_path: Option<PathBuf>,
    /// Exact Rust library test that registers Cases.
    #[arg(long, default_value = "previews::preview")]
    pub test: String,
    #[arg(long = "case", value_name = "CASE")]
    pub case_id: Option<CaseId>,
    #[arg(long)]
    pub list: bool,
    #[arg(long, default_value_t = 600, value_parser = clap::value_parser!(u32).range(64..=4096))]
    pub width: u32,
    #[arg(long, default_value_t = 560, value_parser = clap::value_parser!(u32).range(64..=4096))]
    pub height: u32,
    #[arg(long, default_value = "dark", value_parser = ["dark", "light"])]
    pub theme: String,
    /// Capture the selected case offscreen and exit; requires explicit synthetic fixtures.
    #[arg(long, requires = "case_id")]
    pub capture: Option<PathBuf>,
    /// Compare captured pixels and environment metadata with this baseline.
    #[arg(long, requires = "capture")]
    pub baseline: Option<PathBuf>,
    /// Deliberately replace the baseline after review.
    #[arg(long, requires = "baseline")]
    pub update_baseline: bool,
    #[arg(long)]
    pub no_watch: bool,
}
#[derive(Debug)]
struct SessionDirectory(PathBuf);
impl Drop for SessionDirectory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
impl PreviewCmd {
    fn manifest_cargo(path: &std::path::Path) -> anyhow::Result<Cargo> {
        let resolved = std::fs::canonicalize(path)
            .with_context(|| format!("cannot resolve manifest {}", path.display()))?;
        anyhow::ensure!(
            resolved.is_file(),
            "manifest {} must be a file",
            path.display()
        );
        let directory = resolved
            .parent()
            .with_context(|| format!("manifest {} has no parent directory", path.display()))?;
        Ok(Cargo::new(directory).manifest_path(&resolved))
    }

    pub async fn run(self, ui: &mut Ui) -> anyhow::Result<()> {
        let layout = Layout::resolve();
        let cargo = self
            .manifest_path
            .as_ref()
            .map(|path| Self::manifest_cargo(path))
            .transpose()?
            .unwrap_or_else(|| Cargo::new(&layout.config));
        let build = Build::new(cargo, &self.package).await?;
        let mut changes = build.watch()?;
        ui.step(Step::Building, format!("{} preview cases", self.package));
        let binary = build.compile().await?;
        let root = SessionDirectory(layout.preview_session());
        std::fs::create_dir_all(&root.0)?;
        std::fs::set_permissions(&root.0, std::fs::Permissions::from_mode(0o700))?;
        let mut generation = 1;
        let (mut runner, mut snapshot) = Runner::start(
            &binary,
            &Layout::preview_file(&root.0, "runner-1.sock"),
            &self.test,
            generation,
            self.case_id.as_ref(),
        )
        .await?;
        if self.list {
            for case in &snapshot.cases {
                ui.line(case);
            }
            runner.stop().await;
            return Ok(());
        }
        for asset in omega_renderer::Core::FILES
            .iter()
            .chain(omega_renderer::Preview::FILES)
        {
            AtomicFile::at(Layout::preview_file(&root.0, asset.name))
                .write(asset.contents.as_bytes())?;
        }
        let host_socket = Layout::preview_file(&root.0, "host.sock");
        let listener = tokio::net::UnixListener::bind(&host_socket)?;
        let capture_path = self
            .capture
            .as_ref()
            .map(|_| Layout::preview_file(&root.0, "capture.png"));
        let host_log = Layout::preview_file(&root.0, "host.log");
        AtomicFile::at(&host_log).write(b"")?;
        let mut command = tokio::process::Command::new("quickshell");
        command
            .args(["-p"])
            .arg(Layout::preview_file(&root.0, "shell.qml"))
            .env("OMEGA_PREVIEW_HOST_SOCKET", &host_socket)
            .env("QT_QUICK_CONTROLS_STYLE", "Basic")
            .env("QT_STYLE_OVERRIDE", "Fusion")
            .env("QT_SCALE_FACTOR", "1")
            .env("QT_FONT_DPI", "96")
            .stdout(std::fs::OpenOptions::new().append(true).open(&host_log)?)
            .stderr(std::process::Stdio::inherit())
            .kill_on_drop(true);
        if self.capture.is_some() {
            command
                .env_remove("WAYLAND_DISPLAY")
                .env_remove("DISPLAY")
                .env("QT_QPA_PLATFORM", "offscreen")
                .env("QT_QUICK_BACKEND", "software")
                .env("QT_QPA_PLATFORMTHEME", "generic");
        }
        let mut host = command
            .spawn()
            .context("cannot start Quickshell for the preview")?;
        let stream = tokio::select! {
            result = tokio::time::timeout(Duration::from_secs(15), listener.accept()) => result.context("preview host did not connect")??.0,
            status = host.wait() => anyhow::bail!("preview host exited before connecting: {status:?}\n{}", std::fs::read_to_string(&host_log).unwrap_or_default()),
        };
        anyhow::ensure!(
            stream.peer_cred()?.pid().map(|pid| pid as u32) == host.id(),
            "unexpected preview host peer"
        );
        let (read, write) = stream.into_split();
        let mut host_read = Reader::new(read);
        let mut host_write = Writer::new(write);
        self.decorate(&mut snapshot, capture_path.as_deref());
        host_write.send(&snapshot).await?;
        ui.step(
            Step::Watching,
            format!("{} — isolated previews", self.package),
        );
        let mut rebuilding = tokio::task::JoinSet::new();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
        let outcome = loop {
            tokio::select! {
                message = runner.read.receive::<PreviewSnapshot>(), if !snapshot.stale => {
                    match message {
                        Ok(Some(mut next)) => {
                            self.decorate(&mut next, capture_path.as_deref());
                            snapshot = next;
                            host_write.send(&snapshot).await?;
                        }
                        result => {
                            snapshot.stale = true;
                            snapshot.error = format!("Preview process ended ({result:?}); save to rebuild.");
                            host_write.send(&snapshot).await?;
                        }
                    }
                }
                message = host_read.receive::<PreviewRequest>() => {
                    let Some(request) = message? else { break Ok(()); };
                    if let Some(preview_request::Command::Captured(path)) = &request.command {
                        let Some(expected) = &capture_path else { anyhow::bail!("unexpected capture completion"); };
                        anyhow::ensure!(path == &expected.to_string_lossy(), "capture failed or reported an unexpected path: {path}");
                        break capture::Capture::finish(&self, expected, &snapshot, host.id().context("capture host exited")?, ui).await;
                    }
                    if snapshot.stale {
                        let mut refused = snapshot.clone();
                        refused.answered = request.id;
                        host_write.send(&refused).await?;
                    } else {
                        runner.write.send(&request).await?;
                    }
                }
                Some(()) = changes.next(), if !self.no_watch && self.capture.is_none() && rebuilding.is_empty() => {
                    snapshot.stale = true;
                    snapshot.error = "Building; showing the last successful render.".into();
                    host_write.send(&snapshot).await?;
                    ui.step(Step::Building, &self.package);
                    let build = build.clone();
                    rebuilding.spawn(async move { build.compile().await });
                }
                built = rebuilding.join_next(), if !rebuilding.is_empty() => {
                    let built = built.context("preview build task disappeared")?;
                    let candidate = match built? {
                        Ok(binary) => {
                            generation += 1;
                            match snapshot.selected.parse::<CaseId>() {
                                Ok(case_id) => Runner::start(&binary, &Layout::preview_file(&root.0, format!("runner-{generation}.sock")), &self.test, generation, Some(&case_id)).await,
                                Err(error) => Err(error.into()),
                            }
                        }
                        Err(error) => Err(error),
                    };
                    match candidate {
                        Ok((replacement, mut next)) => {
                            runner.stop().await;
                            runner = replacement;
                            self.decorate(&mut next, None);
                            snapshot = next;
                            ui.step(Step::Built, &self.package);
                        }
                        Err(error) => { snapshot.error = error.to_string(); ui.error(&error); }
                    }
                    host_write.send(&snapshot).await?;
                }
                _ = tokio::time::sleep_until(deadline), if self.capture.is_some() => break Err(anyhow::anyhow!("capture did not settle within 30 seconds")),
                status = host.wait() => break status.context("preview host failed").and_then(|status| { anyhow::ensure!(status.success(), "preview host exited: {status}"); Ok(()) }),
                _ = tokio::signal::ctrl_c() => break Ok(()),
            }
        };
        rebuilding.shutdown().await;
        runner.stop().await;
        let _ = host.kill().await;
        let _ = host.wait().await;
        outcome
    }
    fn decorate(&self, snapshot: &mut PreviewSnapshot, capture: Option<&std::path::Path>) {
        snapshot.width = self.width;
        snapshot.height = self.height;
        snapshot.theme = self.theme.clone();
        snapshot.capture_path = capture
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use omega_host::{
        TempPath,
        cargo::{MetadataRequest, Resolution},
    };

    struct Fixture(PathBuf);

    impl Fixture {
        fn new() -> Self {
            let root = TempPath::sibling(std::path::Path::new("/tmp/omega preview"), "test");
            AtomicFile::at(root.join("Cargo.toml")).write(b"[package]\nname = 'preview-fixture'\nversion = '0.1.0'\nedition = '2024'\n[workspace]\n").unwrap();
            AtomicFile::at(root.join("src/lib.rs")).write(b"").unwrap();
            AtomicFile::at(root.join("README.md"))
                .write(b"not a manifest")
                .unwrap();
            Self(root)
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[tokio::test]
    async fn explicit_manifest_never_falls_back_to_a_neighboring_manifest() {
        let fixture = Fixture::new();
        let request = MetadataRequest::new().resolution(Resolution::Offline);
        let cargo = PreviewCmd::manifest_cargo(&fixture.0.join("Cargo.toml")).unwrap();
        let metadata = cargo.metadata(request).await.unwrap();
        assert_eq!(
            metadata.workspace_packages()[0].name.as_str(),
            "preview-fixture"
        );

        let cargo = PreviewCmd::manifest_cargo(&fixture.0.join("README.md")).unwrap();
        assert!(Build::new(cargo, "preview-fixture").await.is_err());
    }

    #[test]
    fn manifest_paths_must_resolve_to_files() {
        assert!(
            PreviewCmd::manifest_cargo(std::path::Path::new("/"))
                .unwrap_err()
                .to_string()
                .contains("must be a file")
        );
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        assert!(PreviewCmd::manifest_cargo(root).is_err());
        assert!(PreviewCmd::manifest_cargo(&root.join("missing-manifest.toml")).is_err());
        assert!(PreviewCmd::manifest_cargo(&root.join("Cargo.toml")).is_ok());
    }
}
