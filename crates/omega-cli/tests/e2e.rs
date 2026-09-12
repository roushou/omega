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
            .env("OMEGA_SHELL_SOCKET", self.observation())
            .env("OMEGA_SHELL_CONFIG", self.root.join("omarchy/shell.json"));
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
    machine.run(&["new", "battery-widget", "--template", "battery"]);

    // Scaffolding from a checkout links the config to it, so what follows
    // resolves against this tree rather than a crate nobody has published.
    assert!(
        machine.root.join("config/.cargo/config.toml").exists(),
        "init should link a config it can see a checkout for"
    );

    // Adopt an existing mixed layout, then compile the generated Rust itself.
    let shell_path = machine.root.join("omarchy/shell.json");
    std::fs::create_dir_all(shell_path.parent().unwrap()).unwrap();
    std::fs::write(
        &shell_path,
        r#"{
        "version":1,
        "idle":{"screensaver":150,"lock":300},
        "bar":{"position":"top","transparent":false,"layout":{
            "left":[{"id":"omarchy.menu"}],"center":[{"id":"omarchy.clock","format":"HH:mm"}],
            "right":[{"id":"omega.view","unit":"battery-widget","module":"battery-widget"}]
        }},
        "plugins":[],
        "future":{"message":"preserved"}
    }"#,
    )
    .unwrap();
    machine.run(&["shell", "adopt"]);
    let workspace_path = machine.root.join("config/Cargo.toml");
    let wildcard_workspace = std::fs::read_to_string(&workspace_path)
        .unwrap()
        .replace("units/battery-widget", "units/*");
    std::fs::write(&workspace_path, &wildcard_workspace).unwrap();
    let placed = machine.omega(&["new", "extra-widget"]).output().unwrap();
    assert_eq!(
        std::fs::read_to_string(&workspace_path).unwrap(),
        wildcard_workspace
    );

    assert!(placed.status.success());
    let guidance = String::from_utf8_lossy(&placed.stderr);
    assert!(guidance.contains("shell_import.rs"), "{guidance}");
    assert!(guidance.contains("Bar::left"), "{guidance}");
    assert!(guidance.contains("makes it visible"), "{guidance}");

    std::fs::write(machine.root.join("config/system/src/main.rs"),
        "mod shell_import;\nfn main() -> omega_document::Result<()> { omega_document::Document::new().shell(shell_import::shell()?)?.emit()?; Ok(()) }\n").unwrap();
    machine.run(&["check"]);
    machine.run(&["build", "--debug"]);

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
    // The rule a widget colouring a reading has to get right, pinned so the
    // scaffold cannot go back to teaching red for a battery that is filling.
    assert!(
        tested.contains("a_low_charge_on_the_wall_is_not_urgent"),
        "the scaffolded plugin does not pin the urgent rule:\n{tested}"
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

    let lock_path = machine.root.join("config/Cargo.lock");
    let lock_before = std::fs::read(&lock_path).unwrap();
    let provenance = machine.omega(&["status", "--versions"]).output().unwrap();
    assert!(
        provenance.status.success(),
        "{}",
        String::from_utf8_lossy(&provenance.stderr)
    );
    let details = String::from_utf8_lossy(&provenance.stderr);
    assert!(
        details.contains(&format!("CLI {}", env!("CARGO_PKG_VERSION"))),
        "{details}"
    );
    assert!(
        details.contains(&format!("daemon {}", env!("CARGO_PKG_VERSION"))),
        "{details}"
    );
    assert!(details.contains("omega-rs"), "{details}");
    assert!(details.contains("path "), "{details}");
    assert!(details.contains("omega-document"), "{details}");
    assert_eq!(std::fs::read(&lock_path).unwrap(), lock_before);
    assert!(String::from_utf8_lossy(&provenance.stdout).contains("battery-widget"));
    machine.run(&["shell", "apply"]);
    let first = std::fs::read(&shell_path).unwrap();
    assert!(String::from_utf8_lossy(&first).contains("preserved"));
    let changed = String::from_utf8(first.clone())
        .unwrap()
        .replace("HH:mm", "HH:mm:ss");
    std::fs::write(&shell_path, &changed).unwrap();
    let diff = machine.omega(&["shell", "diff"]).output().unwrap();
    assert!(diff.status.success());
    assert!(diff.stdout.is_empty());
    let differences = String::from_utf8_lossy(&diff.stderr);
    assert!(
        differences.contains("/bar/layout/center/0/format"),
        "{differences}"
    );
    assert!(
        differences.contains("current: \"HH:mm:ss\""),
        "{differences}"
    );
    assert!(differences.contains("built:   \"HH:mm\""), "{differences}");
    assert!(!differences.contains("preserved"), "{differences}");
    assert!(differences.contains("apply --overwrite"), "{differences}");
    let refused = machine.omega(&["shell", "apply"]).output().unwrap();
    assert!(
        !refused.status.success(),
        "external edits must require acknowledgement"
    );
    assert_eq!(std::fs::read_to_string(&shell_path).unwrap(), changed);
    let deployment = machine.omega(&["status"]).output().unwrap();
    assert!(deployment.status.success());
    let details = String::from_utf8_lossy(&deployment.stderr);
    assert!(
        details.contains("the daemon has accepted a build"),
        "{details}"
    );
    assert!(details.contains("shell application failed"), "{details}");
    assert!(details.contains("failed:"), "{details}");
    assert!(String::from_utf8_lossy(&deployment.stdout).contains("battery-widget"));
    let waiting = machine
        .omega(&["build", "--debug", "--wait"])
        .output()
        .unwrap();
    assert!(!waiting.status.success());
    let error = String::from_utf8_lossy(&waiting.stderr);
    assert!(error.contains("shell application failed"), "{error}");
    assert!(error.contains("omega shell diff"), "{error}");
    assert_eq!(std::fs::read_to_string(&shell_path).unwrap(), changed);
    machine.run(&["shell", "apply", "--overwrite"]);
    assert_eq!(std::fs::read(&shell_path).unwrap(), first);
    let deployment = machine.omega(&["status"]).output().unwrap();
    assert!(deployment.status.success());
    let details = String::from_utf8_lossy(&deployment.stderr);
    assert!(
        details.contains("last shell application succeeded"),
        "{details}"
    );
    assert!(!details.contains("failed:"), "{details}");

    let source_path = machine.root.join("config/system/src/shell_import.rs");
    let source = std::fs::read_to_string(&source_path).unwrap();
    std::fs::write(&source_path, source.replace("HH:mm", "HH:mm:ss")).unwrap();
    machine.run(&["build", "--debug", "--wait", "--timeout", "30s"]);
    let snapshot = machine.omega(&["status", "--json"]).output().unwrap();
    assert!(
        snapshot.status.success(),
        "{}",
        String::from_utf8_lossy(&snapshot.stderr)
    );
    assert!(snapshot.stderr.is_empty());
    let snapshot: omega_proto::omega::DeploymentStatus =
        serde_json::from_slice(&snapshot.stdout).unwrap();
    let selected = std::fs::read_to_string(machine.root.join("state/current")).unwrap();
    assert_eq!(snapshot.accepted_generation, selected.trim());
    assert_eq!(snapshot.shell_generation, selected.trim());
    assert_eq!(
        snapshot.reconciliation,
        omega_proto::omega::ReconciliationState::Settled as i32
    );
    assert_eq!(
        snapshot.shell,
        omega_proto::omega::ShellApplicationState::Applied as i32
    );
    assert!(
        std::fs::read_to_string(&shell_path)
            .unwrap()
            .contains("HH:mm:ss")
    );
    // Explicitly select the first generation, independent of activation timing.
    let layout = omega_host::Layout::at(
        machine.root.join("config"),
        machine.root.join("state"),
        machine.root.join("cache"),
    );
    let store = omega_host::Generations::new(&layout);
    let first_generation = store
        .recovery_ids()
        .unwrap()
        .into_iter()
        .find(|id| {
            let pinned = store.pin(id).unwrap();
            std::fs::read_to_string(pinned.layout().compiled_shell())
                .unwrap()
                .contains("\"HH:mm\"")
        })
        .expect("first configuration is retained");
    machine.run(&["rollback", first_generation.as_str()]);
    machine.run(&["shell", "apply"]);
    assert_eq!(std::fs::read(&shell_path).unwrap(), first);

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

#[test]
#[ignore = "compiles both scaffold templates with cargo; run with --ignored"]
fn both_templates_build_with_their_printed_placements() {
    let machine = Machine::new();
    machine.run(&["init", "--bare"]);
    let minimal = machine.omega(&["new", "hello-widget"]).output().unwrap();
    let battery = machine
        .omega(&["new", "power-widget", "--template", "battery"])
        .output()
        .unwrap();
    let mut hints = Vec::new();
    for (name, output) in [("hello-widget", minimal), ("power-widget", battery)] {
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stdout.is_empty());
        let output = String::from_utf8_lossy(&output.stderr);
        let hint = omega_cli::scaffold::Scaffold::placement_hint(
            &omega_proto::UnitName::parse(name).unwrap(),
        );
        assert!(output.contains(&hint), "{output}");
        hints.push(hint);
    }
    std::fs::write(
        machine.root.join("config/system/src/main.rs"),
        format!(
            "fn main() -> omega_document::Result<()> {{
                omega_document::Document::new().shell(
                    omega_document::shell::Shell::new().bar(
                        omega_document::shell::Bar::top().right([{}])
                    )
                )?.emit()
            }}",
            hints.join(",")
        ),
    )
    .unwrap();
    machine.run(&["build", "--debug"]);
    let tested = machine.cargo(&["test", "--workspace"]);
    assert!(tested.contains("it_shows_a_greeting"), "{tested}");
    assert!(tested.contains("it_shows_the_charge"), "{tested}");

    let rejected = machine
        .omega(&["new", "invalid-widget", "--template", "unknown"])
        .output()
        .unwrap();
    assert!(!rejected.status.success());
    assert!(!machine.root.join("config/units/invalid-widget").exists());
}

#[test]
#[ignore = "compiles and validates config programs with cargo; run with --ignored"]
fn shell_only_configs_build_and_check_never_publishes() {
    let machine = Machine::new();
    machine.run(&["init", "--bare"]);
    let main = machine.root.join("config/system/src/main.rs");
    let original = std::fs::read_to_string(&main).unwrap();
    machine.run(&["check"]);
    assert!(!machine.root.join("state").exists());
    machine.run(&["build", "--debug"]);
    let layout = omega_host::Layout::at(
        machine.root.join("config"),
        machine.root.join("state"),
        machine.root.join("cache"),
    );
    let generations = omega_host::Generations::new(&layout);
    let baseline = generations.pin_current().unwrap().unwrap();
    let baseline_document = omega_document::DocumentFile::of(baseline.layout())
        .read()
        .unwrap();
    assert!(!baseline_document.shell_json.is_empty());
    let shell_path = machine.root.join("omarchy/shell.json");
    std::fs::create_dir_all(shell_path.parent().unwrap()).unwrap();
    std::fs::write(&shell_path, r#"{"version":1}"#).unwrap();
    machine.run(&["shell", "adopt"]);
    let _daemon = machine.daemon();
    assert!(listening(&machine.observation(), Duration::from_secs(10)));
    machine.run(&["shell", "apply"]);
    assert!(
        std::fs::read_to_string(&shell_path)
            .unwrap()
            .contains("omarchy.menu")
    );

    let baseline_ids = generations.recovery_ids().unwrap();
    let status = machine.omega(&["status"]).output().unwrap();
    assert!(status.status.success());
    assert!(!String::from_utf8_lossy(&status.stderr).contains("omega build"));

    machine.run(&["new", "hello"]);
    for (body, expected) in [
        (
            r#"omega_document::Document::new().shell(
            omega_document::shell::Shell::new().bar(omega_document::shell::Bar::top().right([
                omega_document::shell::PluginWidget::new("hello", "hello").surface("missing").into()
            ])))?.emit()"#,
            "declares no widget",
        ),
        (
            r#"omega_document::Document::new().shell(
            omega_document::shell::Shell::new().bar(omega_document::shell::Bar::top().right([
                omega_document::shell::PluginWidget::new("same", "hello").into(),
                omega_document::shell::PluginWidget::new("same", "hello").into()
            ])))?.emit()"#,
            "duplicate placement",
        ),
        (
            r#"omega_document::Document::new().schedule(omega_document::Schedules::every(
            "tick", omega_document::Cadence::seconds(1),
            omega_document::Actions::invoke("missing", "tick")
        )).emit()"#,
            "unknown unit",
        ),
    ] {
        std::fs::write(
            &main,
            format!("fn main() -> omega_document::Result<()> {{ {body} }}"),
        )
        .unwrap();
        let output = machine.omega(&["check"]).output().unwrap();
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(!output.status.success(), "{stderr}");
        assert!(stderr.contains(expected), "{stderr}");
        assert!(output.stdout.is_empty());
        assert_eq!(
            omega_document::DocumentFile::of(generations.pin_current().unwrap().unwrap().layout())
                .read()
                .unwrap(),
            baseline_document,
        );
        assert_eq!(generations.recovery_ids().unwrap(), baseline_ids);
    }
    std::fs::write(&main, original).unwrap();
    machine.run(&["check"]);
    assert_eq!(generations.recovery_ids().unwrap(), baseline_ids);
}
