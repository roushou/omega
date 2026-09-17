//! Run initialization through the CLI with isolated host commands and real
//! generation publication, daemon reconciliation, and recovery storage.

use omega_host::{
    AtomicFile, Layout, TempPath,
    recovery::{RecoveryStore, Replacement, Snapshot},
};
use omega_proto::Socket;
use std::{
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::Output,
    time::Duration,
};

struct Machine {
    root: PathBuf,
    layout: Layout,
}

impl Machine {
    fn new() -> Self {
        let root = TempPath::sibling(&std::env::temp_dir().join("omega-init"), "test");
        let layout = Layout::at(root.join("config"), root.join("state"), root.join("cache"));
        let machine = Self { root, layout };
        machine.script("rustc", "exit 0");
        machine.script(
            "cargo",
            r#"
echo "cargo $*" >> "$TEST_EVENTS"
[ "$1" = --version ] && exit 0
[ "$TEST_FAILURE" = compile ] && exit 7
mkdir -p "$OMEGA_CONFIG_DIR/target/debug"
cp "$TEST_EMITTER" "$OMEGA_CONFIG_DIR/target/debug/system"
"#,
        );
        machine.script(
            "systemctl",
            r#"
echo "systemctl $*" >> "$TEST_EVENTS"
[ "$TEST_FAILURE" = service ] && [ "$2" = daemon-reload ] && exit 9
if [ "$2" = show ]; then
    printf 'LoadState=loaded\nActiveState=active\nUnitFileState=enabled\nSubState=running\nFragmentPath=%s/omega.service\nNeedDaemonReload=no\n' "$OMEGA_SERVICE_DIR"
fi
exit 0
"#,
        );
        machine.script(
            "omarchy",
            r#"
echo "omarchy $*" >> "$TEST_EVENTS"
[ "$TEST_FAILURE" = restart ] && [ "$1" = restart ] && exit 8
exit 0
"#,
        );
        let shell = omega_omarchy::shell::Shell::new();
        let document = omega_document::Document::new()
            .with(shell)
            .unwrap()
            .into_inner();
        let source = omega_document::DocumentFile::encode(&document).unwrap();
        machine.executable(
            &machine.root.join("emit"),
            &format!("#!/bin/sh\ncat <<'DOCUMENT'\n{source}DOCUMENT\n"),
        );
        machine
    }

    fn executable(&self, path: &Path, source: &str) {
        AtomicFile::at(path)
            .write_with_permissions(source.as_bytes(), std::fs::Permissions::from_mode(0o755))
            .unwrap();
    }

    fn script(&self, name: &str, source: &str) {
        self.executable(
            &self.root.join("bin").join(name),
            &format!("#!/bin/sh\nset -e\n{source}\n"),
        );
    }

    fn command(&self, args: &[&str]) -> tokio::process::Command {
        let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_omega"));
        command
            .args(args)
            .env_clear()
            .env("HOME", &self.root)
            .env(
                "PATH",
                format!("{}:/usr/bin:/bin", self.root.join("bin").display()),
            )
            .env("OMEGA_CONFIG_DIR", &self.layout.config)
            .env("OMEGA_STATE_DIR", &self.layout.state)
            .env("OMEGA_CACHE_DIR", &self.layout.cache)
            .env("OMEGA_SHELL_CONFIG", &self.layout.shell_config)
            .env("OMEGA_SERVICE_DIR", self.root.join("services"))
            .env("OMEGA_SHELL_PLUGINS", self.root.join("renderers"))
            .env("OMEGA_SOCKET", self.socket().path())
            .env("OMEGA_SHELL_SOCKET", self.root.join("shell.sock"))
            .env(
                "OMEGA_SOURCE",
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .parent()
                    .unwrap()
                    .parent()
                    .unwrap(),
            )
            .env("TEST_EVENTS", self.root.join("events"))
            .env("TEST_EMITTER", self.root.join("emit"))
            .kill_on_drop(true);
        command
    }

    async fn run(&self, args: &[&str]) -> String {
        let output = self.command(args).output().await.unwrap();
        assert!(output.status.success(), "{}", Self::diagnostic(&output));
        Self::diagnostic(&output)
    }

    fn diagnostic(output: &Output) -> String {
        format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    }

    fn events(&self) -> String {
        std::fs::read_to_string(self.root.join("events")).unwrap_or_default()
    }

    fn socket(&self) -> Socket {
        Socket::at(self.root.join("control.sock"))
    }

    fn records(&self) -> usize {
        RecoveryStore::new(&self.layout).receipts().unwrap().len()
    }

    async fn daemon(&self) -> (omega_daemon::DaemonHandle, tokio::task::JoinHandle<()>) {
        let daemon = omega_daemon::Daemon::builder(&self.layout)
            .control(self.socket())
            .observation(Socket::at(self.root.join("shell.sock")))
            .build()
            .unwrap();
        let handle = daemon.handle();
        let task = tokio::spawn(async move {
            daemon.run().await.unwrap();
        });
        tokio::time::timeout(Duration::from_secs(3), async {
            while omega_cli::operator::Operator::at(self.socket())
                .daemon_version()
                .await
                .is_err()
            {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        (handle, task)
    }
}

impl Drop for Machine {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[tokio::test]
async fn bare_setup_skips_host_tools_and_preserves_user_sources_on_repeat() {
    let m = Machine::new();
    let output = m.run(&["init", "--bare"]).await;
    assert!(output.contains("--bare skips compilation"));
    assert!(output.contains("setup changes"), "{output}");
    assert!(
        output.contains("Created ~/config/system/src/main.rs"),
        "{output}"
    );
    assert!(output.contains("omega recovery restore"), "{output}");
    let summary = output.split("setup changes").nth(1).unwrap();
    assert!(!summary.contains("Kept ~/config/Cargo.toml"), "{output}");
    assert!(!summary.contains("daemon-reload"), "{output}");
    assert!(m.events().is_empty());
    assert!(!m.root.join("services").exists());
    assert!(!m.layout.active_build().exists());
    AtomicFile::at(m.layout.system_main())
        .write(b"// handwritten source\n")
        .unwrap();
    let records = m.records();
    let repeated = m.run(&["init", "--bare"]).await;
    assert!(
        repeated.contains("Kept ~/config/system/src/main.rs"),
        "{repeated}"
    );
    assert!(!repeated.contains("omega recovery restore"), "{repeated}");
    assert_eq!(m.records(), records);
    assert_eq!(
        std::fs::read_to_string(m.layout.system_main()).unwrap(),
        "// handwritten source\n"
    );
}

#[tokio::test]
async fn missing_tool_and_invalid_workspace_fail_before_installation() {
    let m = Machine::new();
    m.script("cargo", "exit 7");
    let output = m.command(&["init"]).output().await.unwrap();
    assert!(!output.status.success());
    assert!(!m.layout.workspace_manifest().exists());
    assert!(!m.layout.recovery_dir().exists());
    assert!(Machine::diagnostic(&output).contains("cargo is required"));
    let text = Machine::diagnostic(&output);
    assert!(output.stdout.is_empty());
    assert!(text.contains("omega::init::failed"), "{text}");
    assert!(!text.contains("Recovery record:"), "{text}");
    assert!(!text.contains("Completed changes are retained"), "{text}");

    m.script("cargo", "exit 0");
    AtomicFile::at(m.layout.system_manifest())
        .write(b"[broken")
        .unwrap();
    let output = m.command(&["init"]).output().await.unwrap();
    assert!(!output.status.success());
    assert!(!m.layout.workspace_manifest().exists());
    assert!(!m.root.join("services").exists());
    assert!(!m.layout.recovery_dir().exists());
}

#[tokio::test]
async fn compilation_failure_preserves_scaffolding_and_does_not_install_or_publish() {
    let m = Machine::new();
    let output = m
        .command(&["init", "--debug"])
        .env("TEST_FAILURE", "compile")
        .output()
        .await
        .unwrap();
    assert!(!output.status.success());
    let text = Machine::diagnostic(&output);
    assert!(text.contains("compile configuration"), "{text}");
    assert!(text.contains("omega recovery list"), "{text}");
    assert!(text.contains("setup changes"), "{text}");
    assert!(text.contains("omega init --debug"), "{text}");
    assert!(output.stdout.is_empty());
    assert!(m.layout.system_main().exists());
    assert!(m.records() > 0);
    assert!(!m.root.join("services").exists());
    assert!(!m.root.join("renderers").exists());
    assert!(!m.layout.active_build().exists());
    assert!(!m.events().contains("daemon-reload"));
}

#[tokio::test]
async fn service_failure_is_fatal_and_does_not_publish_or_restart_the_shell() {
    let m = Machine::new();
    let output = m
        .command(&["init", "--debug"])
        .env("TEST_FAILURE", "service")
        .output()
        .await
        .unwrap();
    assert!(!output.status.success());
    let text = Machine::diagnostic(&output);
    assert!(text.contains("systemd refused"), "{text}");
    assert!(
        text.contains("systemctl --user status omega.service"),
        "{text}"
    );
    assert!(!text.contains("remains selected"), "{text}");
    assert!(m.root.join("services/omega.service").is_file());
    assert!(!m.layout.active_build().exists());
    assert!(!m.events().contains("omarchy restart"));
    assert!(!text.contains("configuration built and applied"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn initialization_publishes_applies_and_verifies_with_honest_empty_renderer_status() {
    let m = Machine::new();
    let original = b"{\"version\":1,\"bar\":{}}\n";
    AtomicFile::at(&m.layout.shell_config)
        .write(original)
        .unwrap();
    let (handle, running) = m.daemon().await;
    let output = m.run(&["init", "--debug"]).await;
    assert!(
        output.contains("configuration built and applied"),
        "{output}"
    );
    assert!(output.contains("running QML is unverified"), "{output}");
    assert!(output.contains("original shell backup:"), "{output}");
    assert!(output.contains("Restore destination:"), "{output}");
    assert!(
        output.contains("daemon service started and enabled at login"),
        "{output}"
    );
    assert_eq!(std::fs::read(m.layout.shell_backup()).unwrap(), original);
    assert!(m.layout.shell_import().exists());
    let records = m.records();
    let status = omega_cli::operator::Operator::at(m.socket())
        .deployment()
        .await
        .unwrap();
    assert!(!status.accepted_generation.is_empty());
    assert_eq!(
        status.shell,
        omega_proto::omega::ShellApplicationState::Applied as i32
    );
    let again = m.run(&["init", "--debug"]).await;
    assert!(again.contains("configuration built and applied"));
    assert_eq!(
        m.records(),
        records,
        "repeated init must not create backups of unchanged files"
    );
    assert_eq!(std::fs::read(m.layout.shell_backup()).unwrap(), original);
    let events = m.events();
    assert!(events.find("cargo build").unwrap() < events.find("daemon-reload").unwrap());
    assert!(events.find("daemon-reload").unwrap() < events.find("omarchy restart shell").unwrap());
    handle.stop();
    running.await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn shell_restart_failure_keeps_the_published_generation_and_reports_failure() {
    let m = Machine::new();
    let (handle, running) = m.daemon().await;
    let output = m
        .command(&["init", "--debug"])
        .env("TEST_FAILURE", "restart")
        .output()
        .await
        .unwrap();
    assert!(!output.status.success());
    assert!(Machine::diagnostic(&output).contains("shell restart failed"));
    assert!(Machine::diagnostic(&output).contains("remains selected"));
    assert!(Machine::diagnostic(&output).contains("omega rollback"));
    assert!(m.layout.active_build().exists());
    assert!(m.records() > 0);
    handle.stop();
    running.await.unwrap();
}

#[tokio::test]
async fn pending_recovery_blocks_init_and_cli_restore_preserves_subsequent_edits() {
    let m = Machine::new();
    let target = m.layout.system_main();
    let store = RecoveryStore::new(&m.layout);
    let mut saved = store
        .prepare(Replacement::prepare(&target, Snapshot::file(b"new")).unwrap())
        .unwrap();
    let id = saved.receipt().id.to_string();
    drop(saved);
    let output = m.command(&["init", "--bare"]).output().await.unwrap();
    assert!(!output.status.success());
    assert!(!m.layout.workspace_manifest().exists());
    let text = Machine::diagnostic(&output);
    assert!(
        text.contains(&format!("omega recovery inspect {id}")),
        "{text}"
    );
    assert!(!text.contains("setup changes"), "{text}");
    assert!(
        m.run(&["recovery", "inspect", &id])
            .await
            .contains("Before")
    );
    m.run(&["recovery", "restore", &id]).await;
    m.run(&["init", "--bare"]).await;
    saved = store
        .prepare(Replacement::prepare(&target, Snapshot::file(b"new")).unwrap())
        .unwrap();
    saved.apply().unwrap();
    let id = saved.receipt().id.to_string();
    drop(saved);
    AtomicFile::at(&target).write(b"external edit").unwrap();
    let output = m
        .command(&["recovery", "restore", &id])
        .output()
        .await
        .unwrap();
    assert!(!output.status.success());
    assert_eq!(std::fs::read(target).unwrap(), b"external edit");
}

#[tokio::test]
async fn invalid_emitted_document_never_installs_or_publishes() {
    let m = Machine::new();
    m.executable(&m.root.join("emit"), "#!/bin/sh\necho broken\n");
    let output = m.command(&["init", "--debug"]).output().await.unwrap();
    assert!(!output.status.success());
    assert!(Machine::diagnostic(&output).contains("evaluate and validate"));
    assert!(!m.root.join("services").exists());
    assert!(!m.layout.active_build().exists());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "compiles the generated Rust workspace using the real toolchain"]
async fn initialization_compiles_real_rust_and_activates_the_result() {
    let m = Machine::new();
    m.script("cargo", "exec \"$TEST_REAL_CARGO\" \"$@\"");
    m.script("rustc", "exec \"$TEST_REAL_RUSTC\" \"$@\"");
    let sysroot = std::process::Command::new("rustc")
        .args(["--print", "sysroot"])
        .output()
        .unwrap();
    assert!(sysroot.status.success());
    let rustc = Path::new(String::from_utf8_lossy(&sysroot.stdout).trim()).join("bin/rustc");
    let home = PathBuf::from(std::env::var_os("HOME").unwrap());
    let cache = Path::new(env!("CARGO_TARGET_TMPDIR")).join("initialization-target");
    std::fs::create_dir_all(&cache).unwrap();
    std::fs::create_dir_all(&m.layout.config).unwrap();
    std::os::unix::fs::symlink(&cache, m.layout.target_dir()).unwrap();
    let (handle, running) = m.daemon().await;
    let output = m
        .command(&["init", "--debug"])
        .env("TEST_REAL_CARGO", env!("CARGO"))
        .env("TEST_REAL_RUSTC", rustc)
        .env(
            "CARGO_HOME",
            std::env::var_os("CARGO_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join(".cargo")),
        )
        .env(
            "RUSTUP_HOME",
            std::env::var_os("RUSTUP_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join(".rustup")),
        )
        .output()
        .await
        .unwrap();
    handle.stop();
    running.await.unwrap();
    assert!(output.status.success(), "{}", Machine::diagnostic(&output));
    assert!(m.layout.active_build().exists());
    assert!(
        m.layout
            .compiled_system(omega_host::Profile::Debug)
            .is_file()
    );
    for record in std::fs::read_dir(m.layout.recovery_dir()).unwrap() {
        let record = record.unwrap();
        if record.path().extension().is_some_and(|ext| ext == "json") {
            assert!(
                record.metadata().unwrap().len() < 1024 * 1024,
                "recovery must not snapshot the build directory"
            );
        }
    }
}
