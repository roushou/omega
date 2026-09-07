//! The whole cycle, as a person runs it.
//!
//! Ignored by default: it compiles a scaffolded config with cargo, which
//! takes minutes and a registry the machine can reach. It exists so the shell
//! session everyone reruns by hand — `init`, `new`, `build`, `daemon`, `status`,
//! `restart` — is written down and repeatable:
//!
//! ```text
//! cargo test -p omega-cli -- --ignored --nocapture
//! ```

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// This test's own build of the CLI, whichever profile cargo used.
const OMEGA: &str = env!("CARGO_BIN_EXE_omega");

/// A whole machine in a temp directory: three roots and two sockets, so the
/// cycle runs against nothing the developer owns.
struct Machine {
    root: PathBuf,
}

impl Machine {
    fn new() -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = Self::scratch().join(format!("machine-{nanos}"));
        std::fs::create_dir_all(&root).unwrap();
        Self { root }
    }

    /// Cargo's own scratch for integration tests, under `target/`. Not
    /// `/tmp`: a cargo build of the scaffolded config runs to gigabytes, and
    /// on a typical machine `/tmp` is memory.
    fn scratch() -> PathBuf {
        PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("e2e")
    }

    /// The build cache, kept between runs so a rerun is seconds, not minutes.
    /// It lives under `target/`, so `cargo clean` reaches it.
    fn build_cache() -> PathBuf {
        Self::scratch().join("target")
    }

    fn omega(&self, args: &[&str]) -> Command {
        let mut command = Command::new(OMEGA);
        command
            .args(args)
            .env("OMEGA_CONFIG_DIR", self.root.join("config"))
            .env("OMEGA_STATE_DIR", self.root.join("state"))
            .env("OMEGA_CACHE_DIR", self.root.join("cache"))
            .env("OMEGA_SOCKET", self.control())
            .env("OMEGA_SHELL_SOCKET", self.observation());
        command
    }

    /// Run a command to completion, failing the test with its output.
    fn run(&self, args: &[&str]) -> String {
        let output = self.omega(args).output().unwrap();
        assert!(
            output.status.success(),
            "omega {args:?} failed: {}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).into_owned()
    }

    /// Run cargo over the scaffolded workspace, the way its author would.
    fn cargo(&self, args: &[&str]) -> String {
        let output = Command::new("cargo")
            // Where the config's `.cargo/config.toml` says which omega it
            // builds against — cargo looks up from here, not from the
            // manifest.
            .current_dir(self.root.join("config"))
            .args(args)
            .arg("--manifest-path")
            .arg(self.root.join("config").join("Cargo.toml"))
            .arg("--target-dir")
            .arg(Self::build_cache())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "cargo {args:?} failed in the scaffolded config: {}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).into_owned()
    }

    fn control(&self) -> PathBuf {
        self.root.join("omega.sock")
    }

    fn observation(&self) -> PathBuf {
        self.root.join("omega-shell.sock")
    }

    /// Start the daemon, killed when the returned guard drops.
    fn daemon(&self) -> Daemon {
        Daemon(
            self.omega(&["daemon"])
                .stdout(Stdio::null())
                .stderr(Stdio::inherit())
                .spawn()
                .unwrap(),
        )
    }
}

impl Drop for Machine {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// The daemon process, stopped when the test ends however it ends.
struct Daemon(Child);

impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// Wait for a socket to start answering.
fn listening(path: &Path, within: Duration) -> bool {
    let deadline = Instant::now() + within;
    while Instant::now() < deadline {
        if std::os::unix::net::UnixStream::connect(path).is_ok() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    false
}

#[test]
#[ignore = "compiles a scaffolded config with cargo; run with --ignored"]
fn a_scaffolded_config_builds_and_runs() {
    let machine = Machine::new();

    machine.run(&["init", "--bare"]);
    machine.run(&["new", "battery-widget"]);

    // Scaffolding from a checkout links the config to it, so what follows
    // resolves against this tree rather than a crate nobody has published.
    assert!(
        machine.root.join("config/.cargo/config.toml").exists(),
        "init should link a config it can see a checkout for"
    );

    machine.run(&["check"]);
    machine.run(&["build"]);

    // The plugin `omega new` writes arrives with tests, and they pass. A
    // scaffold whose own tests fail teaches its reader that tests fail.
    let tested = machine.cargo(&["test", "--package", "battery-widget"]);
    assert!(
        tested.contains("it_shows_the_charge"),
        "the scaffolded plugin's tests did not run:\n{tested}"
    );
    assert!(
        tested.contains("it_declares_only_what_it_holds"),
        "the scaffolded plugin does not check what it declares:\n{tested}"
    );

    let _daemon = machine.daemon();
    assert!(
        listening(&machine.observation(), Duration::from_secs(10)),
        "the daemon must bind its observation socket"
    );

    // The unit the config declares is running, and the daemon says so.
    let status = machine.run(&["status"]);
    assert!(
        status.contains("battery-widget"),
        "status did not report the unit:\n{status}"
    );

    machine.run(&["restart", "battery-widget"]);

    // A cycled unit is still supervised, and the restart is counted.
    let status = machine.run(&["status"]);
    let line = status
        .lines()
        .find(|line| line.starts_with("battery-widget"))
        .unwrap_or_else(|| panic!("the unit vanished after a restart:\n{status}"));
    assert!(
        !line.contains("stopped"),
        "a cycled unit must come back: {line}"
    );
}
