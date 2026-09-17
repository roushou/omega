use anyhow::{Context, bail};
use omega_proto::{
    omega::{PreviewRequest, PreviewSnapshot, preview_request},
    preview::{CaseId, Reader, VERSION, Writer},
};
use std::{path::Path, time::Duration};
use tokio::{
    net::{
        UnixListener,
        unix::{OwnedReadHalf, OwnedWriteHalf},
    },
    process::Child,
};

#[derive(Debug)]
pub(super) struct Runner {
    pub child: Child,
    pub read: Reader<OwnedReadHalf>,
    pub write: Writer<OwnedWriteHalf>,
}
impl Runner {
    pub(super) async fn start(
        binary: &Path,
        socket: &Path,
        test: &str,
        generation: u32,
        case_id: Option<&CaseId>,
    ) -> anyhow::Result<(Self, PreviewSnapshot)> {
        let listener = UnixListener::bind(socket)?;
        let mut child = tokio::process::Command::new(binary)
            .args(["--exact", test, "--nocapture", "--test-threads=1"])
            .env("OMEGA_PREVIEW_SOCKET", socket)
            .env("OMEGA_PREVIEW_GENERATION", generation.to_string())
            .env_remove("OMEGA_SOCKET")
            .env_remove("OMEGA_SHELL_SOCKET")
            .env_remove(omega_proto::Handshake::TOKEN_ENV)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::inherit())
            .kill_on_drop(true)
            .spawn()?;
        let stream = tokio::select! {
            accepted = tokio::time::timeout(Duration::from_secs(15), listener.accept()) => accepted.context("preview registration did not connect within 15s")??.0,
            status = child.wait() => bail!("preview test exited before registration ({status:?}); add #[test] fn {test}() calling Cases::run"),
        };
        // Only the selected test process can supply this session's catalogue.
        anyhow::ensure!(
            stream.peer_cred()?.pid().map(|pid| pid as u32) == child.id(),
            "preview peer is not the selected test process"
        );
        let (read, write) = stream.into_split();
        let mut runner = Self {
            child,
            read: Reader::new(read),
            write: Writer::new(write),
        };
        let mut snapshot = tokio::time::timeout(
            Duration::from_secs(10),
            runner.read.receive::<PreviewSnapshot>(),
        )
        .await??
        .context("preview closed before its first render")?;
        anyhow::ensure!(
            snapshot.version == VERSION,
            "preview protocol mismatch; link matching omega-preview and CLI versions"
        );
        if let Some(case_id) = case_id {
            runner
                .write
                .send(&PreviewRequest {
                    id: 0,
                    command: Some(preview_request::Command::Select(case_id.to_string())),
                })
                .await?;
            snapshot = tokio::time::timeout(Duration::from_secs(10), runner.read.receive())
                .await??
                .context("preview closed while selecting a case")?;
            anyhow::ensure!(snapshot.error.is_empty(), "{}", snapshot.error);
        }
        Ok((runner, snapshot))
    }
    pub(super) async fn stop(&mut self) {
        // Test runtimes have no production session to drain; killing drops isolated work.
        let _ = self.child.kill().await;
        let _ = self.child.wait().await;
    }
}
