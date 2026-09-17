use super::{BuildRequest, Metadata, MetadataRequest, TestArtifacts, TestBuildRequest};
use crate::process::{OutputLimits, Process};
use cargo_metadata::Message;
use std::{
    io,
    path::PathBuf,
    process::{ExitStatus, Stdio},
};
use tokio::{
    io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, BufReader},
    process::Command,
};

/// Run Cargo in an explicit working directory, using `cargo` from PATH by default.
/// Construction performs no I/O. The environment and local Cargo configuration are
/// inherited; the `CARGO` environment variable does not override this handle's executable.
/// Relative working directories are resolved when an operation runs. Prefer absolute
/// executable overrides; relative executable paths containing separators are platform-dependent.
///
/// Operations have no deadline. Dropping their futures kills the direct Cargo child;
/// this does not guarantee that descendant processes stop or undo lockfile/build writes.
/// Callers own workspace locking and may wrap operations in a timeout.
///
/// ```no_run
/// use omega_host::cargo::{Cargo, BuildRequest, Selection, MetadataRequest, Resolution};
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let cargo = Cargo::new("/home/me/desktop");
/// cargo.build(BuildRequest::new(Selection::Workspace)).await?;
/// let metadata = cargo.metadata(
///     MetadataRequest::new().resolution(Resolution::OfflineLocked),
/// ).await?;
/// let test_cargo = Cargo::new("/tmp/fixture").executable("/tmp/fake-cargo");
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone)]
pub struct Cargo {
    directory: PathBuf,
    executable: PathBuf,
    manifest_path: Option<PathBuf>,
}

impl Cargo {
    pub fn new(directory: impl Into<PathBuf>) -> Self {
        Self {
            directory: directory.into(),
            executable: "cargo".into(),
            manifest_path: None,
        }
    }

    /// Override the executable without changing the workspace or process environment.
    pub fn executable(mut self, executable: impl Into<PathBuf>) -> Self {
        self.executable = executable.into();
        self
    }

    /// Select the exact manifest for all operations. Cargo validates the file.
    /// Relative paths resolve from this handle's working directory. This does not
    /// change that directory or where Cargo discovers local configuration.
    /// Without an override, Cargo discovers the manifest from the working directory.
    pub fn manifest_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.manifest_path = Some(path.into());
        self
    }

    /// Compile selected packages, inheriting stdout and stderr for live build output.
    /// A nonzero exit is an error; diagnostics have already been written by Cargo.
    pub async fn build(&self, request: BuildRequest) -> Result<(), InvocationError> {
        let mut command = self.command("build");
        request.apply(&mut command);
        let status = Process::new(command)
            .status()
            .await
            .map_err(|e| self.execution("build", e))?;
        Self::success("build", status, String::new())
    }

    /// Read metadata format 1. Capture is limited to 64 MiB stdout and 64 KiB stderr;
    /// exceeding either limit is an error. Unknown metadata fields are permitted.
    pub async fn metadata(&self, request: MetadataRequest) -> Result<Metadata, InvocationError> {
        let mut command = self.command("metadata");
        command.arg("--format-version=1");
        request.resolution.apply(&mut command);
        let output = Process::new(command)
            .capture(OutputLimits {
                stdout: 64 * 1024 * 1024,
                stderr: 64 * 1024,
            })
            .await
            .map_err(|e| self.execution("metadata", e))?;
        Self::success(
            "metadata",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim().into(),
        )?;
        serde_json::from_slice(&output.stdout).map_err(|source| InvocationError::Decode {
            operation: "metadata",
            source,
        })
    }

    /// Compile one package's library tests without running them. Read compiler messages
    /// incrementally, with at most 8 MiB per line. Cargo stderr is inherited; rendered
    /// compiler diagnostics are returned (also on failure), capped at 64 KiB.
    /// Non-JSON lines are diagnostics; malformed known messages fail decoding. Unknown
    /// message reasons are ignored for forward compatibility.
    pub async fn compile_tests(
        &self,
        request: TestBuildRequest,
    ) -> Result<TestArtifacts, InvocationError> {
        let mut command = self.command("test");
        request.apply(&mut command);
        let mut child = command
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|e| self.execution("test", crate::process::Error::Spawn(e)))?;
        let stdout = child.stdout.take().expect("piped stdout");
        let mut artifacts = TestArtifacts::new(request.package);
        Self::messages(stdout, &mut artifacts).await?;
        let status = child
            .wait()
            .await
            .map_err(|e| self.execution("test", crate::process::Error::Wait(e)))?;
        Self::success("test", status, artifacts.diagnostics().to_owned())?;
        Ok(artifacts)
    }

    fn command(&self, operation: &'static str) -> Command {
        let mut command = Command::new(&self.executable);
        command
            .current_dir(&self.directory)
            .arg(operation)
            .stdin(Stdio::null())
            .kill_on_drop(true);
        if let Some(path) = &self.manifest_path {
            command.arg("--manifest-path").arg(path);
        }
        command
    }

    fn execution(&self, operation: &'static str, source: crate::process::Error) -> InvocationError {
        InvocationError::Execution {
            operation,
            executable: self.executable.clone(),
            directory: self.directory.clone(),
            source,
        }
    }

    fn success(
        operation: &'static str,
        status: ExitStatus,
        diagnostics: String,
    ) -> Result<(), InvocationError> {
        if status.success() {
            Ok(())
        } else {
            Err(InvocationError::Failed {
                operation,
                status,
                diagnostics,
            })
        }
    }

    async fn messages(
        stdout: impl AsyncRead + Unpin,
        artifacts: &mut TestArtifacts,
    ) -> Result<(), InvocationError> {
        const LIMIT: u64 = 8 * 1024 * 1024;
        let mut reader = BufReader::new(stdout);
        let mut line = Vec::new();
        loop {
            line.clear();
            let count = (&mut reader)
                .take(LIMIT + 1)
                .read_until(b'\n', &mut line)
                .await
                .map_err(InvocationError::Messages)?;
            if count == 0 {
                return Ok(());
            }
            if count as u64 > LIMIT {
                return Err(InvocationError::Messages(io::Error::other(
                    "Cargo message exceeded 8 MiB",
                )));
            }
            Self::message(&line, artifacts)?;
        }
    }

    fn message(line: &[u8], artifacts: &mut TestArtifacts) -> Result<(), InvocationError> {
        #[derive(serde::Deserialize)]
        struct Header {
            reason: String,
        }

        if !line
            .iter()
            .find(|byte| !byte.is_ascii_whitespace())
            .is_some_and(|byte| *byte == b'{')
        {
            artifacts.diagnostic(&String::from_utf8_lossy(line));
            return Ok(());
        }
        let decode = |source| InvocationError::Decode {
            operation: "test",
            source,
        };
        let header: Header = serde_json::from_slice(line).map_err(decode)?;
        if !matches!(
            header.reason.as_str(),
            "compiler-artifact" | "compiler-message" | "build-finished" | "build-script-executed"
        ) {
            return Ok(());
        }
        match serde_json::from_slice::<Message>(line).map_err(decode)? {
            Message::CompilerArtifact(artifact) => artifacts.record(artifact),
            Message::CompilerMessage(message) => {
                artifacts.diagnostic(&message.to_string());
                artifacts.diagnostic("\n");
            }
            Message::BuildFinished(_) | Message::BuildScriptExecuted(_) => {}
            Message::TextLine(line) => artifacts.diagnostic(&line),
            _ => {}
        }
        Ok(())
    }
}

/// Failure to execute Cargo, decode its output, or complete a requested operation.
#[derive(Debug, thiserror::Error)]
pub enum InvocationError {
    #[error("cannot run {} {operation} in {}: {source}", executable.display(), directory.display())]
    Execution {
        operation: &'static str,
        executable: PathBuf,
        directory: PathBuf,
        source: crate::process::Error,
    },
    #[error("cargo {operation} failed ({status})\n{diagnostics}")]
    Failed {
        operation: &'static str,
        status: ExitStatus,
        diagnostics: String,
    },
    #[error("invalid cargo {operation} output: {source}")]
    Decode {
        operation: &'static str,
        source: serde_json::Error,
    },
    #[error("cannot read Cargo compiler messages: {0}")]
    Messages(#[source] io::Error),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cargo::{ArtifactError, PackageId, PackageSpec, Resolution, Selection};
    use crate::{AtomicFile, Profile, TempPath};
    use serde_json::json;
    use std::{os::unix::fs::symlink, path::Path, time::Duration};

    struct Fixture {
        root: PathBuf,
        script: PathBuf,
    }

    impl Fixture {
        fn new(body: &str) -> Self {
            let root = TempPath::sibling(Path::new("/tmp/omega cargo"), "test");
            let script = root.join("fake-cargo");
            let source = format!(
                "#!/bin/sh\npwd > \"$0.cwd\"\nprintf '%s\\n' \"$@\" > \"$0.args\"\n{body}\n"
            );
            AtomicFile::at(&script).write(source.as_bytes()).unwrap();
            // Keep scripts as interpreter inputs, avoiding execution of freshly written inodes.
            symlink("/bin/sh", root.join("cargo")).unwrap();
            for operation in ["build", "metadata", "test"] {
                AtomicFile::at(root.join(operation))
                    .write(format!("exec /bin/sh ./fake-cargo {operation} \"$@\"\n").as_bytes())
                    .unwrap();
            }
            Self { root, script }
        }

        fn cargo(&self) -> Cargo {
            Cargo::new(&self.root).executable(self.root.join("cargo"))
        }

        fn output(&self, bytes: &[u8]) {
            AtomicFile::at(self.script.with_extension("output"))
                .write(bytes)
                .unwrap();
        }

        fn arguments(&self) -> String {
            std::fs::read_to_string(self.script.with_extension("args")).unwrap()
        }

        fn package() -> PackageId {
            serde_json::from_value(json!("path+file:///workspace#example@0.1.0")).unwrap()
        }

        fn artifact(package: &str, kind: &str, test: bool, executable: &str) -> String {
            json!({
                "reason": "compiler-artifact", "package_id": package,
                "target": {"name": "custom_library_name", "kind": [kind], "crate_types": [kind], "src_path": "/workspace/src/lib.rs", "edition": "2024", "doc": true, "doctest": true, "test": true},
                "profile": {"opt_level": "0", "debug_assertions": true, "overflow_checks": true, "test": test},
                "features": [], "filenames": [], "executable": executable, "fresh": false
            }).to_string()
        }

        async fn pid(&self) -> i32 {
            tokio::time::timeout(Duration::from_secs(5), async {
                loop {
                    if let Ok(pid) = std::fs::read_to_string(self.script.with_extension("pid"))
                        && let Ok(pid) = pid.trim().parse()
                    {
                        return pid;
                    }
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
            })
            .await
            .expect("child started")
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    #[tokio::test]
    async fn builds_preserve_directory_and_explicit_selection_profile_and_target() {
        let fixture = Fixture::new("exit 0");
        fixture
            .cargo()
            .build(
                BuildRequest::new(Selection::Package(
                    "example@0.1.0".parse::<PackageSpec>().unwrap(),
                ))
                .profile(Profile::Release)
                .target_dir("target with spaces")
                .resolution(Resolution::Locked),
            )
            .await
            .unwrap();
        assert_eq!(
            fixture.arguments(),
            "build\n--package=example@0.1.0\n--release\n--target-dir\ntarget with spaces\n--locked\n"
        );
        assert_eq!(
            std::fs::read_to_string(fixture.script.with_extension("cwd"))
                .unwrap()
                .trim(),
            fixture.root.to_str().unwrap()
        );

        fixture
            .cargo()
            .build(BuildRequest::new(Selection::Workspace))
            .await
            .unwrap();
        assert_eq!(fixture.arguments(), "build\n--workspace\n");
        assert_eq!(
            Cargo::new(&fixture.root)
                .command("build")
                .as_std()
                .get_program(),
            "cargo"
        );
    }

    #[tokio::test]
    async fn manifest_selection_is_preserved_for_every_operation() {
        let fixture = Fixture::new("cat \"$0.output\"");
        let cargo = fixture.cargo().manifest_path("nested project/Cargo.toml");
        fixture.output(b"");
        cargo
            .build(BuildRequest::new(Selection::Workspace))
            .await
            .unwrap();
        assert!(
            fixture
                .arguments()
                .starts_with("build\n--manifest-path\nnested project/Cargo.toml\n")
        );

        fixture.output(json!({"packages": [], "workspace_members": [], "workspace_root": "/workspace", "target_directory": "/build", "version": 1}).to_string().as_bytes());
        cargo.metadata(MetadataRequest::new()).await.unwrap();
        assert!(
            fixture
                .arguments()
                .starts_with("metadata\n--manifest-path\nnested project/Cargo.toml\n")
        );

        fixture.output(b"");
        cargo
            .compile_tests(TestBuildRequest::library(Fixture::package()))
            .await
            .unwrap();
        assert!(
            fixture
                .arguments()
                .starts_with("test\n--manifest-path\nnested project/Cargo.toml\n")
        );
        assert_eq!(
            std::fs::read_to_string(fixture.script.with_extension("cwd"))
                .unwrap()
                .trim(),
            fixture.root.to_str().unwrap()
        );
    }

    #[tokio::test]
    async fn metadata_decodes_and_propagates_resolution_policy() {
        let fixture = Fixture::new("cat \"$0.output\"");
        fixture.output(json!({"packages": [], "workspace_members": [], "workspace_root": "/workspace", "target_directory": "/build", "version": 1, "future": true}).to_string().as_bytes());
        let metadata = fixture
            .cargo()
            .metadata(MetadataRequest::new().resolution(Resolution::OfflineLocked))
            .await
            .unwrap();
        assert_eq!(metadata.workspace_root, "/workspace");
        assert_eq!(
            fixture.arguments(),
            "metadata\n--format-version=1\n--offline\n--locked\n"
        );
        fixture.output(b"{broken");
        assert!(matches!(
            fixture.cargo().metadata(MetadataRequest::new()).await,
            Err(InvocationError::Decode { .. })
        ));
    }

    #[tokio::test]
    async fn failed_commands_keep_exit_status_and_diagnostics() {
        let fixture = Fixture::new("echo missing-dependency >&2\nexit 7");
        let error = fixture
            .cargo()
            .metadata(MetadataRequest::new())
            .await
            .unwrap_err();
        assert!(
            matches!(error, InvocationError::Failed { status, ref diagnostics, .. } if status.code() == Some(7) && diagnostics.contains("missing-dependency"))
        );

        let fixture = Fixture::new("echo compiler-failure\nexit 9");
        let error = fixture
            .cargo()
            .compile_tests(TestBuildRequest::library(Fixture::package()))
            .await
            .unwrap_err();
        assert!(
            matches!(error, InvocationError::Failed { status, ref diagnostics, .. } if status.code() == Some(9) && diagnostics.contains("compiler-failure"))
        );
    }

    #[tokio::test]
    async fn test_artifacts_match_package_identity_and_refuse_ambiguity() {
        let fixture = Fixture::new("cat \"$0.output\"");
        let package = Fixture::package();
        let wanted = Fixture::artifact(&package.to_string(), "rlib", true, "/target/wanted");
        let messages = [
            "procedural macro output".into(),
            r#"{"reason":"future-message","value":42}"#.into(),
            wanted.clone(),
            wanted.clone(),
            Fixture::artifact(
                "path+file:///dependency#example@0.2.0",
                "lib",
                true,
                "/target/dependency",
            ),
            Fixture::artifact(&package.to_string(), "lib", false, "/target/library"),
            Fixture::artifact(&package.to_string(), "bin", true, "/target/binary"),
        ]
        .join("\n");
        fixture.output(messages.as_bytes());
        let artifacts = fixture
            .cargo()
            .compile_tests(TestBuildRequest::library(package.clone()))
            .await
            .unwrap();
        assert_eq!(
            artifacts.library_test().unwrap(),
            Path::new("/target/wanted")
        );
        assert!(artifacts.diagnostics().contains("procedural macro output"));
        assert_eq!(
            fixture.arguments(),
            format!("test\n--package={package}\n--lib\n--no-run\n--message-format=json\n")
        );

        fixture.output(
            format!(
                "{wanted}\n{}",
                Fixture::artifact(&package.to_string(), "lib", true, "/target/other")
            )
            .as_bytes(),
        );
        let artifacts = fixture
            .cargo()
            .compile_tests(TestBuildRequest::library(package.clone()))
            .await
            .unwrap();
        assert!(matches!(
            artifacts.library_test(),
            Err(ArtifactError::Ambiguous(_))
        ));

        fixture.output(b"");
        let artifacts = fixture
            .cargo()
            .compile_tests(TestBuildRequest::library(package))
            .await
            .unwrap();
        assert!(matches!(
            artifacts.library_test(),
            Err(ArtifactError::Missing(_))
        ));
    }

    #[tokio::test]
    async fn malformed_known_messages_and_excessive_output_fail() {
        let fixture = Fixture::new("cat \"$0.output\"");
        fixture.output(br#"{"reason":"compiler-artifact"}"#);
        assert!(matches!(
            fixture
                .cargo()
                .compile_tests(TestBuildRequest::library(Fixture::package()))
                .await,
            Err(InvocationError::Decode { .. })
        ));

        fixture.output(&vec![b'x'; 8 * 1024 * 1024 + 1]);
        assert!(matches!(
            fixture
                .cargo()
                .compile_tests(TestBuildRequest::library(Fixture::package()))
                .await,
            Err(InvocationError::Messages(_))
        ));
    }

    #[tokio::test]
    async fn cancelling_each_operation_kills_and_reaps_its_cargo_child() {
        for operation in ["build", "metadata", "test"] {
            let fixture = Fixture::new("echo $$ > \"$0.pid\"\nexec sleep 60");
            let cargo = fixture.cargo();
            let task = tokio::spawn(async move {
                match operation {
                    "build" => {
                        cargo
                            .build(BuildRequest::new(Selection::Workspace))
                            .await
                            .unwrap();
                    }
                    "metadata" => {
                        cargo.metadata(MetadataRequest::new()).await.unwrap();
                    }
                    "test" => {
                        cargo
                            .compile_tests(TestBuildRequest::library(Fixture::package()))
                            .await
                            .unwrap();
                    }
                    _ => unreachable!(),
                }
            });
            let pid = fixture.pid().await;
            task.abort();
            assert!(task.await.unwrap_err().is_cancelled());
            tokio::time::timeout(Duration::from_secs(5), async {
                while unsafe { libc::kill(pid, 0) } == 0 {
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
            })
            .await
            .expect("Cargo child exited and was reaped");
        }
    }

    #[tokio::test]
    async fn real_cargo_respects_local_configuration_and_never_runs_library_tests() {
        let fixture = Fixture::new("exit 0");
        AtomicFile::at(fixture.root.join("Cargo.toml"))
            .write(
                br#"
[package]
name = "example"
version = "0.1.0"
edition = "2024"
[lib]
name = "renamed_library"
[workspace]
"#,
            )
            .unwrap();
        AtomicFile::at(fixture.root.join("src/lib.rs"))
            .write(b"#[test] fn must_not_run() { panic!(\"compilation must not execute tests\"); }")
            .unwrap();
        AtomicFile::at(fixture.root.join(".cargo/config.toml"))
            .write(b"[build]\ntarget-dir = 'configured-target'\n")
            .unwrap();
        let cargo = Cargo::new(&fixture.root)
            .manifest_path(fixture.root.join("Cargo.toml"))
            .executable(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()));
        let metadata = cargo
            .metadata(MetadataRequest::new().resolution(Resolution::Offline))
            .await
            .unwrap();
        let package = metadata.workspace_packages()[0].id.clone();
        let artifacts = cargo
            .compile_tests(
                TestBuildRequest::library(package.clone()).resolution(Resolution::OfflineLocked),
            )
            .await
            .unwrap();
        assert!(
            artifacts
                .library_test()
                .unwrap()
                .starts_with(fixture.root.join("configured-target"))
        );
        assert!(artifacts.library_test().unwrap().is_file());
        let artifacts = cargo
            .compile_tests(
                TestBuildRequest::library(package.clone())
                    .target_dir(fixture.root.join("override-target"))
                    .resolution(Resolution::OfflineLocked),
            )
            .await
            .unwrap();
        assert!(
            artifacts
                .library_test()
                .unwrap()
                .starts_with(fixture.root.join("override-target"))
        );

        AtomicFile::at(fixture.root.join("src/lib.rs"))
            .write(b"compile_error!(\"fixture compilation failed\");")
            .unwrap();
        let error = cargo
            .compile_tests(TestBuildRequest::library(package).resolution(Resolution::OfflineLocked))
            .await
            .unwrap_err();
        assert!(matches!(
            error,
            InvocationError::Failed { operation: "test", status, diagnostics }
                if !status.success() && diagnostics.contains("fixture compilation failed")
        ));
    }
}
