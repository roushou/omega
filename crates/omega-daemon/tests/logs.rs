//! A unit's own output, kept where a crash can be read after the restart.

use std::path::PathBuf;
use std::time::Duration;

use omega_daemon::Shutdown;
use omega_daemon::hub::Hub;
use omega_daemon::manifest::ManifestStore;
use omega_daemon::supervisor::{Supervisor, UnitLog, UnitSpec};
use omega_daemon::units::UnitTable;
use omega_proto::Socket;
use omega_proto::UnitName;

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

    fn log(&self, name: &str) -> UnitLog {
        UnitLog::at(self.0.join(format!("{name}.log")))
    }

    /// A runnable stand-in for a compiled unit.
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
async fn a_units_output_survives_the_restart_that_follows_it() {
    let tmp = TempDir::new("crash");
    let log = tmp.log("noisy");
    let (supervisor, shutdown) = supervisor("crash");

    // A unit that says why it is dying, then dies.
    let script = tmp.executable(
        "noisy",
        "#!/bin/sh\necho 'the manifest hash did not match' >&2\nexit 2\n",
    );

    supervisor.spawn(UnitSpec::new(UnitName::parse("noisy").unwrap(), &script).logged(log.clone()));

    // The message is on disk after the process is gone, which is the whole
    // point: `omega status` says it exited, the log says why.
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

    // A crash-looping unit writes the same thing forever.
    std::fs::write(log.path(), "x".repeat(UnitLog::MAX_BYTES as usize + 1)).unwrap();

    // Opening for the next run starts it over rather than filling the disk.
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

/// A table holding the manifests a test declares, which is what a supervisor
/// now needs instead of a manifest store of its own.
fn table_with(manifests: ManifestStore) -> UnitTable {
    let units = UnitTable::detached(Hub::new());
    units.adopt(&manifests);
    units
}
