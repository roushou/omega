use super::{Status, StatusError, UnitName};
use crate::process::{Error as ProcessError, OutputLimits, Process};
use std::{path::PathBuf, process::ExitStatus, time::Duration};

/// The systemd manager addressed by every operation on a handle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    User,
    System,
}

impl Scope {
    fn argument(self) -> &'static str {
        match self {
            Self::User => "--user",
            Self::System => "--system",
        }
    }
}

/// An asynchronous systemctl client. Construction has no effects and resolves no
/// paths or environment. Defaults to `systemctl` on PATH, a 30-second deadline,
/// and at most 64 KiB on each output stream. All operations propagate errors.
///
/// Dropping an operation kills its local systemctl process. A submitted systemd
/// job may still complete: cancellation and timeouts never imply rollback.
#[derive(Debug, Clone)]
pub struct Manager {
    scope: Scope,
    executable: PathBuf,
    timeout: Duration,
}

impl Manager {
    pub fn new(scope: Scope) -> Self {
        Self {
            scope,
            executable: "systemctl".into(),
            timeout: Duration::from_secs(30),
        }
    }

    pub fn executable(mut self, path: impl Into<PathBuf>) -> Self {
        self.executable = path.into();
        self
    }

    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn scope(&self) -> Scope {
        self.scope
    }

    /// Verify that this manager can be contacted, without inspecting environment contents.
    pub async fn probe(&self) -> Result<(), ManagerError> {
        self.run(&["show", "--property=Version", "--value"], None)
            .await
            .map(|_| ())
    }

    pub async fn reload(&self) -> Result<(), ManagerError> {
        self.run(&["daemon-reload"], None).await.map(|_| ())
    }

    pub fn diagnose_command(&self, unit: Option<&UnitName>) -> String {
        format!(
            "systemctl {} status{}",
            self.scope.argument(),
            unit.map(|name| {
                if name.as_str().contains('\\') {
                    format!(" '{name}'")
                } else {
                    format!(" {name}")
                }
            })
            .unwrap_or_default()
        )
    }

    pub(super) async fn status(&self, name: &UnitName) -> Result<Status, ManagerError> {
        let output = self
            .run(&["show", "--all", Status::PROPERTIES], Some(name))
            .await?;
        let source = std::str::from_utf8(&output).map_err(|source| ManagerError::Encoding {
            unit: name.clone(),
            diagnose: self.diagnose_command(Some(name)),
            source,
        })?;
        Status::from_show_output(source).map_err(|source| ManagerError::Status {
            unit: name.clone(),
            diagnose: self.diagnose_command(Some(name)),
            source,
        })
    }

    pub(super) async fn command(&self, name: &UnitName, args: &[&str]) -> Result<(), ManagerError> {
        self.run(args, Some(name)).await.map(|_| ())
    }

    async fn run(&self, args: &[&str], name: Option<&UnitName>) -> Result<Vec<u8>, ManagerError> {
        let mut arguments = vec![self.scope.argument()];
        arguments.extend_from_slice(args);
        if let Some(name) = name {
            arguments.extend(["--", name.as_str()]);
        }
        let command = format!("systemctl {}", arguments.join(" "));
        let diagnose = self.diagnose_command(name);
        let mut process = tokio::process::Command::new(&self.executable);
        process
            .args(&arguments)
            .env("LC_ALL", "C")
            .env("SYSTEMD_COLORS", "0")
            .env("SYSTEMD_PAGER", "");
        let output = Process::new(process)
            .timeout(self.timeout)
            .capture(OutputLimits {
                stdout: 64 * 1024,
                stderr: 64 * 1024,
            })
            .await
            .map_err(|source| match source {
                ProcessError::Timeout { duration } => ManagerError::Timeout {
                    command: command.clone(),
                    diagnose: diagnose.clone(),
                    timeout: duration,
                },
                source => ManagerError::Execution {
                    command: command.clone(),
                    diagnose: diagnose.clone(),
                    source,
                },
            })?;
        if !output.status.success() {
            return Err(ManagerError::Refused {
                command,
                diagnose,
                status: output.status,
                stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
            });
        }
        Ok(output.stdout)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ManagerError {
    #[error("could not run {command}: {source}; inspect {diagnose}")]
    Execution {
        command: String,
        diagnose: String,
        #[source]
        source: ProcessError,
    },
    #[error("systemd refused {command} ({status}): {stderr}; inspect {diagnose}")]
    Refused {
        command: String,
        diagnose: String,
        status: ExitStatus,
        stderr: String,
    },
    #[error(
        "{command} timed out after {timeout:?}; the systemd job may still complete; inspect {diagnose} before retrying"
    )]
    Timeout {
        command: String,
        diagnose: String,
        timeout: Duration,
    },
    #[error("cannot read systemd status for {unit}: {source}; inspect {diagnose}")]
    Status {
        unit: UnitName,
        diagnose: String,
        #[source]
        source: StatusError,
    },
    #[error("systemd returned non-UTF-8 status for {unit}: {source}; inspect {diagnose}")]
    Encoding {
        unit: UnitName,
        diagnose: String,
        #[source]
        source: std::str::Utf8Error,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AtomicFile, TempPath, systemd::Service};
    use std::{os::unix::fs::PermissionsExt, path::Path};

    struct Fixture {
        executable: PathBuf,
        root: PathBuf,
    }

    impl Fixture {
        fn new(body: &str) -> Self {
            let root = TempPath::sibling(Path::new("/tmp/omega-systemd"), "test");
            let executable = root.join("systemctl");
            let source = format!("#!/bin/sh\nprintf '%s\\n' \"$@\" > \"$0.args\"\n{body}\n");
            AtomicFile::at(&executable)
                .write_with_permissions(source.as_bytes(), std::fs::Permissions::from_mode(0o755))
                .unwrap();
            Self { executable, root }
        }

        fn manager(&self, scope: Scope) -> Manager {
            Manager::new(scope).executable(&self.executable)
        }

        fn service(&self, scope: Scope) -> Service {
            Service::new(
                self.manager(scope),
                "example.service".parse::<UnitName>().unwrap(),
                self.root.join("example.service"),
            )
            .unwrap()
        }

        fn arguments(&self) -> Vec<String> {
            std::fs::read_to_string(self.executable.with_extension("args"))
                .unwrap()
                .lines()
                .map(str::to_owned)
                .collect()
        }

        async fn pid(&self) -> i32 {
            tokio::time::timeout(Duration::from_secs(3), async {
                loop {
                    if let Ok(source) =
                        std::fs::read_to_string(self.executable.with_extension("pid"))
                        && let Ok(pid) = source.trim().parse()
                    {
                        return pid;
                    }
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
            })
            .await
            .expect("child started")
        }

        async fn exited(&self, pid: i32) {
            tokio::time::timeout(Duration::from_secs(3), async {
                // A read-only liveness check; no signal is delivered.
                while unsafe { libc::kill(pid, 0) } == 0 {
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
            })
            .await
            .expect("local systemctl child exited and was reaped");
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    #[tokio::test]
    async fn manager_scope_and_service_identity_are_explicit_arguments() {
        let fixture = Fixture::new("exit 0");
        let service = fixture.service(Scope::User);
        service.enable(true).await.unwrap();
        assert_eq!(
            fixture.arguments(),
            ["--user", "enable", "--now", "--", "example.service"]
        );
        fixture.service(Scope::System).restart().await.unwrap();
        assert_eq!(
            fixture.arguments(),
            ["--system", "restart", "--", "example.service"]
        );
        service.manager().reload().await.unwrap();
        assert_eq!(fixture.arguments(), ["--user", "daemon-reload"]);
    }

    #[tokio::test]
    async fn failures_preserve_exit_status_stderr_and_target() {
        let fixture = Fixture::new("printf 'permission denied\\ndetail\\n' >&2\nexit 9");
        let error = fixture.service(Scope::User).start().await.unwrap_err();
        let ManagerError::Refused {
            status,
            stderr,
            diagnose,
            ..
        } = error
        else {
            panic!("wrong error: {error}")
        };
        assert_eq!(status.code(), Some(9));
        assert_eq!(stderr, "permission denied\ndetail");
        assert_eq!(diagnose, "systemctl --user status example.service");
    }

    #[tokio::test]
    async fn invalid_status_is_an_error_even_when_the_command_succeeds() {
        let fixture = Fixture::new("printf 'ActiveState=inactive\\n'");
        assert!(matches!(
            fixture.service(Scope::User).status().await,
            Err(ManagerError::Status { .. })
        ));
        assert!(fixture.arguments().contains(&"--all".to_string()));
    }

    #[tokio::test]
    async fn deadlines_kill_the_local_process_and_report_uncertain_remote_outcome() {
        let fixture = Fixture::new("printf '%s\\n' \"$$\" > \"$0.pid\"\nexec /bin/sleep 60");
        let manager = fixture
            .manager(Scope::User)
            .timeout(Duration::from_secs(10));
        let task = tokio::spawn(async move { manager.reload().await });
        let pid = fixture.pid().await;
        tokio::time::pause();
        tokio::time::advance(Duration::from_secs(11)).await;
        let error = task.await.unwrap().unwrap_err();
        tokio::time::resume();
        assert!(matches!(error, ManagerError::Timeout { .. }));
        assert!(error.to_string().contains("may still complete"));
        fixture.exited(pid).await;
    }

    #[tokio::test]
    async fn cancelling_an_operation_kills_its_local_process() {
        let fixture = Fixture::new("printf '%s\\n' \"$$\" > \"$0.pid\"\nexec /bin/sleep 60");
        let manager = fixture.manager(Scope::User);
        let task = tokio::spawn(async move { manager.reload().await });
        let pid = fixture.pid().await;
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
        fixture.exited(pid).await;
    }

    #[tokio::test]
    async fn output_is_bounded_on_both_streams() {
        for redirect in ["", " >&2"] {
            let fixture = Fixture::new(&format!("while :; do printf '%01024d' 0{redirect}; done"));
            let error = fixture.manager(Scope::User).reload().await.unwrap_err();
            assert!(
                matches!(
                    error,
                    ManagerError::Execution {
                        source: ProcessError::OutputLimit { limit: 65536, .. },
                        ..
                    }
                ),
                "{error}"
            );
        }
    }
}
