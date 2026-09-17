//! End-to-end CLI tests using a scaffolded config and real binaries.
//! Ignored by default because they compile generated workspaces and may access the registry.
//!
//! ```text
//! cargo test -p omega-cli -- --ignored --nocapture
//! ```

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// This test's own build of the CLI, whichever profile cargo used.
const OMEGA: &str = env!("CARGO_BIN_EXE_omega");

/// Isolated config, state, and cache roots with two test sockets.
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

    /// Use Cargo target scratch space for generated builds, which may require gigabytes.
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
    fn cargo_command(&self, args: &[&str]) -> Command {
        let mut command = Command::new("cargo");
        command
            // Cargo discovers local patches relative to its working directory.
            .current_dir(self.root.join("config"))
            .args(args)
            .arg("--manifest-path")
            .arg(self.root.join("config").join("Cargo.toml"))
            .arg("--target-dir")
            .arg(Self::build_cache());
        command
    }

    fn cargo(&self, args: &[&str]) -> String {
        let output = self.cargo_command(args).output().unwrap();
        assert!(
            output.status.success(),
            "cargo {args:?} failed in the scaffolded config: {}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).into_owned()
    }

    fn assert_members(&self, patterns: &[&str], packages: &[&str]) {
        let source = std::fs::read_to_string(self.root.join("config/Cargo.toml")).unwrap();
        let manifest: toml::Value = toml::from_str(&source).unwrap();
        let members = manifest["workspace"]["members"].as_array().unwrap();
        assert_eq!(
            members
                .iter()
                .map(|m| m.as_str().unwrap())
                .collect::<Vec<_>>(),
            patterns
        );
        let output = Command::new("cargo")
            .current_dir(self.root.join("config"))
            .args([
                "metadata",
                "--no-deps",
                "--offline",
                "--format-version",
                "1",
            ])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let metadata: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        let mut actual: Vec<_> = metadata["packages"]
            .as_array()
            .unwrap()
            .iter()
            .map(|package| package["name"].as_str().unwrap())
            .collect();
        actual.sort_unstable();
        assert_eq!(actual, packages);
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
fn scaffolding_adds_membership_globs_as_each_directory_gets_its_first_crate() {
    for library_first in [false, true] {
        let machine = Machine::new();
        machine.run(&["init", "--bare"]);
        machine.assert_members(&["system"], &["system"]);
        let (first, second, first_glob, second_glob, first_package) = if library_first {
            (
                vec!["new", "shared-types", "--lib"],
                vec!["new", "hello-widget"],
                "crates/*",
                "plugins/*",
                "shared-types",
            )
        } else {
            (
                vec!["new", "hello-widget"],
                vec!["new", "shared-types", "--lib"],
                "plugins/*",
                "crates/*",
                "hello-widget",
            )
        };
        machine.run(&first);
        machine.assert_members(&["system", first_glob], &[first_package, "system"]);
        machine.run(&second);
        machine.assert_members(
            &["system", first_glob, second_glob],
            &["hello-widget", "shared-types", "system"],
        );
        let manifest = machine.root.join("config/Cargo.toml");
        let before = std::fs::read(&manifest).unwrap();
        machine.run(&["new", "another-widget"]);
        machine.run(&["new", "more-types", "--lib"]);
        machine.run(&["init", "--bare"]);
        assert_eq!(std::fs::read(&manifest).unwrap(), before);
        machine.assert_members(
            &["system", first_glob, second_glob],
            &[
                "another-widget",
                "hello-widget",
                "more-types",
                "shared-types",
                "system",
            ],
        );
        let layout = omega_host::Layout::at(
            machine.root.join("config"),
            machine.root.join("state"),
            machine.root.join("cache"),
        );
        let plugins = omega_host::workspace::Plugins::discover(&layout).unwrap();
        assert_eq!(
            plugins.iter().map(|name| name.as_str()).collect::<Vec<_>>(),
            ["another-widget", "hello-widget"]
        );
    }
}

#[test]
#[ignore = "compiles a scaffolded config with cargo; run with --ignored"]
fn a_scaffolded_config_builds_and_runs() {
    let machine = Machine::new();

    machine.run(&["init", "--bare"]);
    machine.run(&["new", "battery-widget", "--template", "battery"]);

    // Use checkout patches for the scaffolded config.
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
    let wildcard_workspace = std::fs::read_to_string(&workspace_path).unwrap();
    assert!(wildcard_workspace.contains("\"plugins/*\""));
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
        "mod shell_import;\nfn main() -> omega_document::Result<()> { omega_document::Document::new().with(shell_import::shell()?)?.emit()?; Ok(()) }\n").unwrap();
    machine.run(&[
        "new",
        "desktop-ui",
        "--lib",
        "--into",
        "plugins/extra-widget",
        "--into",
        "system",
    ]);
    std::fs::write(
        machine.root.join("config/crates/desktop-ui/src/lib.rs"),
        "pub struct Desktop; impl Desktop { pub const NAME: &str = \"my desktop\"; }\n",
    )
    .unwrap();
    let extra = machine.root.join("config/plugins/extra-widget/src/lib.rs");
    let source =
        std::fs::read_to_string(&extra).unwrap() + "\nconst _: &str = desktop_ui::Desktop::NAME;\n";
    std::fs::write(extra, source).unwrap();
    let system = machine.root.join("config/system/src/main.rs");
    let source = std::fs::read_to_string(&system).unwrap()
        + "\nconst _: &str = desktop_ui::Desktop::NAME;\n";
    std::fs::write(system, source).unwrap();
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
    let output = machine.omega(&["status"]).output().unwrap();
    assert!(output.status.success());
    assert!(output.stdout.is_empty());
    let status = String::from_utf8_lossy(&output.stderr);
    assert!(
        !status.contains("desktop-ui"),
        "libraries must never be supervised"
    );
    assert!(
        status.contains("battery-widget"),
        "status did not report the unit:\n{status}"
    );

    let details = machine
        .omega(&["status", "battery-widget"])
        .output()
        .unwrap();
    assert!(details.status.success());
    assert!(details.stdout.is_empty());
    let details = String::from_utf8_lossy(&details.stderr);
    assert!(details.contains("Process:"), "{details}");
    assert!(details.contains("battery-widget.log"), "{details}");

    let snapshot = machine.run(&["status", "battery-widget", "--json"]);
    let snapshot: omega_proto::omega::DeploymentStatus = serde_json::from_str(&snapshot).unwrap();
    assert_eq!(snapshot.units.len(), 1);
    assert_eq!(snapshot.plugins.len(), 1);
    assert_eq!(snapshot.plugins[0].unit, "battery-widget");

    let unknown = machine
        .omega(&["status", "does-not-exist"])
        .output()
        .unwrap();
    assert!(!unknown.status.success());
    assert!(unknown.stdout.is_empty());
    assert!(String::from_utf8_lossy(&unknown.stderr).contains("unknown plugin does-not-exist"));

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
    assert!(provenance.stdout.is_empty());
    assert!(details.contains("battery-widget"));
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
    assert!(deployment.stdout.is_empty());
    assert!(details.contains("battery-widget"));
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
    let output = machine.omega(&["status"]).output().unwrap();
    assert!(output.status.success());
    assert!(output.stdout.is_empty());
    let status = String::from_utf8_lossy(&output.stderr);
    let line = status
        .lines()
        .find(|line| line.split_whitespace().nth(1) == Some("battery-widget"))
        .unwrap_or_else(|| panic!("the unit vanished after a restart:\n{status}"));
    assert!(
        !line.contains("Stopped"),
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
    for (name, output, template) in [
        (
            "hello-widget",
            minimal,
            omega_cli::scaffold::Template::Minimal,
        ),
        (
            "power-widget",
            battery,
            omega_cli::scaffold::Template::Battery,
        ),
    ] {
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stdout.is_empty());
        let output = String::from_utf8_lossy(&output.stderr);
        let hint = omega_cli::scaffold::Scaffold::placement_hint(
            &omega_host::package::PackageName::parse(name).unwrap(),
            template,
        );
        assert!(output.contains(&hint), "{output}");
        hints.push(hint);
    }
    std::fs::write(
        machine.root.join("config/system/src/main.rs"),
        format!(
            "fn main() -> omega_document::Result<()> {{
                omega_document::Document::new().with(
                    omega_omarchy::shell::Shell::new().bar(
                        omega_omarchy::shell::Bar::top().right([{}])
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
    assert!(!machine.root.join("config/plugins/invalid-widget").exists());
}

#[test]
#[ignore = "compiles and validates config programs with cargo; run with --ignored"]
fn shell_only_configs_build_and_check_never_publishes() {
    let machine = Machine::new();
    machine.run(&["init", "--bare"]);
    let main = machine.root.join("config/system/src/main.rs");
    let original = std::fs::read_to_string(&main).unwrap();
    let state = machine.root.join("state");
    let initialized = omega_host::recovery::Snapshot::read(&state).unwrap();
    machine.run(&["check"]);
    assert_eq!(
        omega_host::recovery::Snapshot::read(&state).unwrap(),
        initialized,
        "checking must preserve initialization recovery records without publishing"
    );
    let layout = omega_host::Layout::at(
        machine.root.join("config"),
        state,
        machine.root.join("cache"),
    );
    assert!(!layout.generations_dir().exists());
    machine.run(&["build", "--debug"]);
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
            r#"omega_document::Document::new().with(
            omega_omarchy::shell::Shell::new().bar(omega_omarchy::shell::Bar::top().right([
                omega_omarchy::shell::PluginWidget::named("hello", "hello").surface_named("missing").into()
            ])))?.emit()"#,
            "declares no widget",
        ),
        (
            r#"omega_document::Document::new().with(
            omega_omarchy::shell::Shell::new().bar(omega_omarchy::shell::Bar::top().right([
                omega_omarchy::shell::PluginWidget::named("same", "hello").into(),
                omega_omarchy::shell::PluginWidget::named("same", "hello").into()
            ])))?.emit()"#,
            "duplicate placement",
        ),
        (
            r#"omega_document::Document::new().schedule(omega_document::Schedules::every(
            "tick", omega_document::Cadence::seconds(1),
            omega_document::Actions::invoke_named("missing", "tick")
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

#[test]
fn generated_workspace_builds_stay_in_cargo_target_scratch_space() {
    let machine = Machine::new();
    let scratch = Path::new(env!("CARGO_TARGET_TMPDIR"));
    assert!(machine.root.starts_with(scratch));
    assert!(Machine::build_cache().starts_with(scratch));
    let command = machine.cargo_command(&["test", "--workspace"]);
    assert_eq!(command.get_program(), "cargo");
    assert_eq!(
        command.get_current_dir(),
        Some(machine.root.join("config").as_path())
    );
    let args: Vec<_> = command.get_args().collect();
    let target = args.iter().position(|arg| *arg == "--target-dir").unwrap();
    assert_eq!(Path::new(args[target + 1]), Machine::build_cache());
}
