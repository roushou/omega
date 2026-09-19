use super::Signal;
use std::io;

/// Shared subprocess creation and shutdown for supervised Omega executables.
pub(crate) struct ManagedChild;

impl ManagedChild {
    pub(crate) const GRACE: std::time::Duration = std::time::Duration::from_secs(5);

    pub(crate) fn spawn(
        program: &std::path::Path,
        socket: &omega_proto::Socket,
        token: &crate::plugins::SpawnToken,
        generation: Option<&omega_host::Generation>,
        log: Option<&crate::supervisor::PluginLog>,
    ) -> io::Result<tokio::process::Child> {
        let mut command = tokio::process::Command::new(program);
        command
            .env("OMEGA_SOCKET", socket.path())
            .env(omega_proto::Handshake::TOKEN_ENV, token.as_str())
            .kill_on_drop(true);
        if let Some(generation) = generation {
            generation.protect_child(command.as_std_mut());
        }
        if let Some(log) = log {
            let (out, err) = log.streams()?;
            command.stdout(out).stderr(err);
        }
        command.spawn()
    }

    /// Return only after the child is reaped, or report an OS wait failure.
    pub(crate) async fn stop(
        child: &mut tokio::process::Child,
    ) -> io::Result<std::process::ExitStatus> {
        if let Some(pid) = child.id()
            && let Err(error) = Signal::terminate(pid as i32)
        {
            tracing::debug!(pid, %error, "could not send SIGTERM; killing child");
            child.start_kill()?;
        }
        match tokio::time::timeout(Self::GRACE, child.wait()).await {
            Ok(result) => result,
            Err(_) => {
                child.start_kill()?;
                child.wait().await
            }
        }
    }
}
