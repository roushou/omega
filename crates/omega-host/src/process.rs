//! Execute short-lived commands with bounded output and direct-child cancellation.

use std::{
    fmt,
    future::Future,
    io,
    process::{ExitStatus, Output, Stdio},
    time::Duration,
};
use tokio::{
    io::{AsyncRead, AsyncReadExt},
    process::{Child, Command},
};

/// Maximum captured bytes on each stream. Exceeding either limit fails execution;
/// output is never silently truncated. A zero limit accepts only an empty stream.
#[derive(Debug, Clone, Copy)]
pub struct OutputLimits {
    pub stdout: usize,
    pub stderr: usize,
}

/// A command's output stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stream {
    Stdout,
    Stderr,
}

impl fmt::Display for Stream {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Stdout => "stdout",
            Self::Stderr => "stderr",
        })
    }
}

/// Execute a Tokio command without interpreting its exit status or output.
/// Construction performs no I/O. Arguments, environment, and working directory
/// remain as configured on the command. Stdin is closed; output handling is chosen
/// by [`Self::capture`] or [`Self::status`]. There is no timeout by default.
///
/// Execution errors and timeouts kill and reap the direct child before returning.
/// Dropping the future requests a direct-child kill; Tokio reaps it on a best-effort
/// basis. Descendants are not supervised, and external effects are not rolled back.
/// A timeout covers waiting and pipe draining after spawn, excluding kill/reap cleanup.
///
/// ```no_run
/// use omega_host::process::{OutputLimits, Process};
/// use std::time::Duration;
/// use tokio::process::Command;
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let mut command = Command::new("rustc");
/// command.arg("--version");
/// let output = Process::new(command)
///     .timeout(Duration::from_secs(10))
///     .capture(OutputLimits { stdout: 64 * 1024, stderr: 64 * 1024 })
///     .await?;
/// assert!(output.status.success());
/// # Ok(())
/// # }
/// ```
#[derive(Debug)]
pub struct Process {
    command: Command,
    timeout: Option<Duration>,
}

impl Process {
    pub fn new(command: Command) -> Self {
        Self {
            command,
            timeout: None,
        }
    }

    /// Limit waiting and output draining to this duration after spawning.
    pub fn timeout(mut self, duration: Duration) -> Self {
        self.timeout = Some(duration);
        self
    }

    /// Inherit stdout and stderr and wait for exit, returning nonzero statuses too.
    pub async fn status(mut self) -> Result<ExitStatus, Error> {
        self.command
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit());
        let mut child = self.spawn()?;
        let result = Self::within(self.timeout, async {
            child.wait().await.map_err(Error::Wait)
        })
        .await;
        Self::finish(&mut child, result).await
    }

    /// Drain stdout and stderr concurrently within explicit byte limits.
    /// Success requires child exit and EOF on both pipes, including on nonzero exit.
    pub async fn capture(mut self, limits: OutputLimits) -> Result<Output, Error> {
        self.command.stdout(Stdio::piped()).stderr(Stdio::piped());
        let mut child = self.spawn()?;
        let stdout = child.stdout.take().expect("piped stdout");
        let stderr = child.stderr.take().expect("piped stderr");

        let result = Self::within(self.timeout, async {
            let (status, stdout, stderr) = tokio::try_join!(
                async { child.wait().await.map_err(Error::Wait) },
                Self::read(stdout, Stream::Stdout, limits.stdout),
                Self::read(stderr, Stream::Stderr, limits.stderr),
            )?;
            Ok(Output {
                status,
                stdout,
                stderr,
            })
        })
        .await;
        Self::finish(&mut child, result).await
    }

    fn spawn(&mut self) -> Result<Child, Error> {
        self.command
            .stdin(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(Error::Spawn)
    }

    async fn within<T>(
        timeout: Option<Duration>,
        future: impl Future<Output = Result<T, Error>>,
    ) -> Result<T, Error> {
        match timeout {
            Some(duration) => tokio::time::timeout(duration, future)
                .await
                .map_err(|_| Error::Timeout { duration })?,
            None => future.await,
        }
    }

    async fn finish<T>(child: &mut Child, result: Result<T, Error>) -> Result<T, Error> {
        if let Err(cause) = result {
            if let Err(source) = child.kill().await {
                return Err(Error::Cleanup {
                    cause: Box::new(cause),
                    source,
                });
            }
            return Err(cause);
        }
        result
    }

    async fn read(
        reader: impl AsyncRead + Unpin,
        stream: Stream,
        limit: usize,
    ) -> Result<Vec<u8>, Error> {
        let mut bytes = Vec::new();
        reader
            .take((limit as u64).saturating_add(1))
            .read_to_end(&mut bytes)
            .await
            .map_err(|source| Error::Read { stream, source })?;
        if bytes.len() > limit {
            return Err(Error::OutputLimit { stream, limit });
        }
        Ok(bytes)
    }
}

/// Execution failure. A nonzero child exit is returned as data, not as this error.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("could not start subprocess: {0}")]
    Spawn(#[source] io::Error),
    #[error("could not wait for subprocess: {0}")]
    Wait(#[source] io::Error),
    #[error("could not read subprocess {stream}: {source}")]
    Read { stream: Stream, source: io::Error },
    #[error("subprocess {stream} exceeded {limit} bytes")]
    OutputLimit { stream: Stream, limit: usize },
    #[error("subprocess timed out after {duration:?}")]
    Timeout { duration: Duration },
    #[error("{cause}; also could not kill/reap subprocess: {source}")]
    Cleanup {
        cause: Box<Error>,
        source: io::Error,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TempPath;
    use std::path::{Path, PathBuf};

    struct Fixture {
        pid_file: PathBuf,
    }

    impl Fixture {
        const LIMITS: OutputLimits = OutputLimits {
            stdout: 128 * 1024,
            stderr: 128 * 1024,
        };

        fn new() -> Self {
            Self {
                pid_file: TempPath::sibling(Path::new("/tmp/omega-process.pid"), "test"),
            }
        }

        fn command(script: &str) -> Command {
            let mut command = Command::new("/bin/sh");
            command.args(["-c", script, "omega-process-test"]);
            command
        }

        fn running(&self, script: &str) -> Process {
            let mut command = Self::command(&format!("echo $$ > \"$1\"; {script}"));
            command.arg(&self.pid_file);
            Process::new(command)
        }

        async fn pid(&self) -> i32 {
            tokio::time::timeout(Duration::from_secs(5), async {
                loop {
                    if let Ok(text) = std::fs::read_to_string(&self.pid_file)
                        && let Ok(pid) = text.trim().parse()
                    {
                        return pid;
                    }
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
            })
            .await
            .expect("child wrote its pid")
        }

        fn assert_reaped(pid: i32) {
            assert_eq!(unsafe { libc::kill(pid, 0) }, -1, "child still exists");
            assert_eq!(io::Error::last_os_error().raw_os_error(), Some(libc::ESRCH));
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.pid_file);
        }
    }

    #[tokio::test]
    async fn capture_preserves_bytes_status_and_command_configuration() {
        let mut command = Fixture::command(
            "read ignored && exit 1; printf '%s' \"$1\"; printf '\\377%s' \"$OMEGA_PROCESS_TEST\" >&2; pwd; exit 7",
        );
        command
            .arg("literal $(false); value ")
            .env("OMEGA_PROCESS_TEST", "error")
            .current_dir("/tmp");
        let output = Process::new(command)
            .capture(Fixture::LIMITS)
            .await
            .unwrap();
        assert_eq!(output.status.code(), Some(7));
        assert_eq!(output.stdout, b"literal $(false); value /tmp\n");
        assert_eq!(output.stderr, b"\xfferror");
    }

    #[tokio::test]
    async fn both_pipes_are_drained_without_waiting_for_exit() {
        let command = Fixture::command("head -c 131072 /dev/zero >&2; head -c 131072 /dev/zero");
        let output = Process::new(command)
            .timeout(Duration::from_secs(5))
            .capture(Fixture::LIMITS)
            .await
            .unwrap();
        assert!(output.status.success());
        assert_eq!(output.stdout, vec![0; 128 * 1024]);
        assert_eq!(output.stderr, vec![0; 128 * 1024]);
    }

    #[tokio::test]
    async fn limits_accept_exactly_the_budget_including_zero() {
        let output = Process::new(Fixture::command("printf abc"))
            .capture(OutputLimits {
                stdout: 3,
                stderr: 0,
            })
            .await
            .unwrap();
        assert_eq!(output.stdout, b"abc");
        let error = Process::new(Fixture::command("printf x >&2"))
            .capture(OutputLimits {
                stdout: 0,
                stderr: 0,
            })
            .await
            .unwrap_err();
        assert!(matches!(
            error,
            Error::OutputLimit {
                stream: Stream::Stderr,
                limit: 0
            }
        ));
    }

    #[tokio::test]
    async fn excessive_output_kills_and_reaps_the_child_on_either_stream() {
        for (redirect, expected) in [("", Stream::Stdout), (" >&2", Stream::Stderr)] {
            let fixture = Fixture::new();
            let error = fixture
                .running(&format!("while :; do printf 12345{redirect}; done"))
                .timeout(Duration::from_secs(5))
                .capture(OutputLimits {
                    stdout: 4,
                    stderr: 4,
                })
                .await
                .unwrap_err();
            assert!(matches!(error, Error::OutputLimit { stream, limit: 4 } if stream == expected));
            Fixture::assert_reaped(fixture.pid().await);
        }
    }

    #[tokio::test]
    async fn timeouts_kill_and_reap_for_capture_and_status() {
        for capture in [true, false] {
            let fixture = Fixture::new();
            let process = fixture
                .running("exec /bin/sleep 60")
                .timeout(Duration::from_secs(10));
            let task = tokio::spawn(async move {
                if capture {
                    process.capture(Fixture::LIMITS).await.map(|_| ())
                } else {
                    process.status().await.map(|_| ())
                }
            });
            let pid = fixture.pid().await;
            tokio::time::pause();
            tokio::time::advance(Duration::from_secs(11)).await;
            let error = task.await.unwrap().unwrap_err();
            tokio::time::resume();
            assert!(matches!(error, Error::Timeout { .. }));
            Fixture::assert_reaped(pid);
        }
    }

    #[tokio::test]
    async fn dropping_capture_and_status_kills_the_direct_child() {
        for capture in [true, false] {
            let fixture = Fixture::new();
            let process = fixture.running("exec /bin/sleep 60");
            let task = tokio::spawn(async move {
                if capture {
                    process.capture(Fixture::LIMITS).await.map(|_| ())
                } else {
                    process.status().await.map(|_| ())
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
            .expect("cancelled child was reaped");
            Fixture::assert_reaped(pid);
        }
    }

    #[tokio::test]
    async fn status_preserves_nonzero_exit_and_spawn_errors_are_distinct() {
        let status = Process::new(Fixture::command("exit 9"))
            .status()
            .await
            .unwrap();
        assert_eq!(status.code(), Some(9));
        let command = Command::new("/dev/null/not-an-executable");
        assert!(matches!(
            Process::new(command).capture(Fixture::LIMITS).await,
            Err(Error::Spawn(_))
        ));
    }
}
