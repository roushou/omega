//! A plugin's own output, kept where a crash can be read after the restart.

use std::path::PathBuf;
use std::time::Duration;

use omega_daemon::Shutdown;
use omega_daemon::hub::Hub;
use omega_daemon::manifest::ManifestStore;
use omega_daemon::plugins::PluginRegistry;
use omega_daemon::supervisor::{PluginLog, PluginSpec, Supervisor};
use omega_proto::PluginName;
use omega_proto::Socket;

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("omega-logs-{tag}-{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    fn log(&self, name: &str) -> PluginLog {
        PluginLog::at(self.0.join(format!("{name}.log")))
    }

    /// A runnable stand-in for a compiled plugin.
    fn executable(&self, name: &str, script: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;

        let path = self.0.join(format!("{name}.sh"));
        std::fs::write(&path, script).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn supervisor(tag: &str) -> (Supervisor, Shutdown) {
    let shutdown = Shutdown::new();
    let supervisor = Supervisor::new(
        Socket::at(format!("/tmp/omega-logs-{tag}.sock")),
        table_with(ManifestStore::default()),
        shutdown.clone(),
    );
    (supervisor, shutdown)
}

#[tokio::test]
async fn a_plugins_output_survives_the_restart_that_follows_it() {
    let tmp = TempDir::new("crash");
    let log = tmp.log("noisy");
    let (supervisor, shutdown) = supervisor("crash");

    // A plugin that says why it is dying, then dies.
    let script = tmp.executable(
        "noisy",
        "#!/bin/sh\necho 'the manifest hash did not match' >&2\nexit 2\n",
    );

    supervisor.spawn(
        PluginSpec::new("noisy".parse::<PluginName>().unwrap(), &script).logged(log.clone()),
    );

    // Plugin logs must remain readable after process exit.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    let mut contents = String::new();
    while tokio::time::Instant::now() < deadline {
        contents = std::fs::read_to_string(log.path()).unwrap_or_default();
        if contents.contains("did not match") {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    shutdown.trigger();
    assert!(contents.contains("did not match"), "{contents:?}");
}

#[test]
fn a_log_that_outgrows_its_cap_starts_over() {
    let tmp = TempDir::new("cap");
    let log = tmp.log("chatty");

    // A crash-looping plugin writes the same thing forever.
    std::fs::write(log.path(), "x".repeat(PluginLog::MAX_BYTES as usize + 1)).unwrap();

    // Truncate oversized logs at process restart.
    drop(log.open().unwrap());
    assert_eq!(std::fs::metadata(log.path()).unwrap().len(), 0);
}

#[test]
fn a_log_within_its_cap_is_appended_to() {
    let tmp = TempDir::new("append");
    let log = tmp.log("steady");
    std::fs::write(log.path(), "first run\n").unwrap();

    let mut file = log.open().unwrap();
    std::io::Write::write_all(&mut file, b"second run\n").unwrap();

    assert_eq!(
        std::fs::read_to_string(log.path()).unwrap(),
        "first run\nsecond run\n"
    );
}

/// Seed the supervisor’s authoritative table with the test manifests.
fn table_with(manifests: ManifestStore) -> PluginRegistry {
    let plugins = PluginRegistry::detached(Hub::new());
    plugins.adopt(&manifests);
    plugins
}
