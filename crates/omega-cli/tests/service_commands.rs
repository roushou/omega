//! Exercise service commands through the CLI with an isolated systemctl executable.

use omega_host::{AtomicFile, Layout, TempPath, recovery::RecoveryStore};
use std::{
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    process::{Command, Output},
};

struct Machine {
    root: PathBuf,
    layout: Layout,
}

impl Machine {
    fn new() -> Self {
        let root = TempPath::sibling(&std::env::temp_dir().join("omega-service"), "test");
        let layout = Layout::at(root.join("config"), root.join("state"), root.join("cache"));
        AtomicFile::at(root.join("bin/systemctl"))
            .write_with_permissions(
                br#"#!/bin/sh
printf '%s\n' "$*" >> "$TEST_EVENTS"
if [ "$2" = "$TEST_FAILURE" ]; then
    printf 'operation rejected\nunderlying systemd detail\n' >&2
    exit 9
fi
if [ "$2" = show ]; then
    active=${TEST_STATE:-active}
    [ "$TEST_INACTIVE" = 1 ] && active=inactive
    printf 'LoadState=loaded\nActiveState=%s\nUnitFileState=enabled\nSubState=running\nFragmentPath=%s/omega.service\nNeedDaemonReload=no\n' "$active" "$OMEGA_SERVICE_DIR"
fi
exit 0
"#,
                std::fs::Permissions::from_mode(0o755),
            )
            .unwrap();
        Self { root, layout }
    }

    fn command(&self, action: &str) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_omega"));
        command
            .args(["daemon", action])
            .env_clear()
            .env("HOME", &self.root)
            .env("PATH", self.root.join("bin"))
            .env("OMEGA_CONFIG_DIR", &self.layout.config)
            .env("OMEGA_STATE_DIR", &self.layout.state)
            .env("OMEGA_CACHE_DIR", &self.layout.cache)
            .env("OMEGA_SERVICE_DIR", self.root.join("services"))
            .env("OMEGA_SOCKET", self.root.join("control.sock"))
            .env("TEST_EVENTS", self.root.join("events"))
            .env("TEST_INACTIVE", "0");
        command
    }

    fn unit(&self) -> PathBuf {
        self.root.join("services/omega.service")
    }

    fn events(&self) -> Vec<String> {
        std::fs::read_to_string(self.root.join("events"))
            .unwrap_or_default()
            .lines()
            .map(ToOwned::to_owned)
            .collect()
    }

    fn failure(output: &Output, operation: &str) {
        let diagnostic = String::from_utf8_lossy(&output.stderr);
        assert!(!output.status.success(), "{diagnostic}");
        assert!(output.stdout.is_empty());
        assert!(diagnostic.contains(operation), "{diagnostic}");
        assert!(diagnostic.contains("operation rejected"), "{diagnostic}");
        assert!(
            diagnostic.contains("underlying systemd detail"),
            "{diagnostic}"
        );
        assert!(
            diagnostic.contains("systemctl --user status omega.service"),
            "{diagnostic}"
        );
        assert!(!diagnostic.contains("the daemon is running, and runs at every login"));
        assert!(!diagnostic.contains("the daemon runs at the next login"));
    }
}

impl Drop for Machine {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[test]
fn installation_stops_at_each_rejected_operation_and_retains_recoverable_files() {
    for (operation, expected) in [
        (
            "daemon-reload",
            vec![
                "--user show --all --property=LoadState,ActiveState,UnitFileState,SubState,FragmentPath,NeedDaemonReload -- omega.service",
                "--user daemon-reload",
            ],
        ),
        (
            "enable",
            vec![
                "--user show --all --property=LoadState,ActiveState,UnitFileState,SubState,FragmentPath,NeedDaemonReload -- omega.service",
                "--user daemon-reload",
                "--user enable --now -- omega.service",
            ],
        ),
        (
            "restart",
            vec![
                "--user show --all --property=LoadState,ActiveState,UnitFileState,SubState,FragmentPath,NeedDaemonReload -- omega.service",
                "--user daemon-reload",
                "--user enable --now -- omega.service",
                "--user restart -- omega.service",
            ],
        ),
    ] {
        let machine = Machine::new();
        let output = machine
            .command("install")
            .env("TEST_FAILURE", operation)
            .output()
            .unwrap();
        Machine::failure(&output, operation);
        assert_eq!(machine.events(), expected);
        assert!(machine.unit().is_file());
        assert_eq!(
            RecoveryStore::new(&machine.layout)
                .receipts()
                .unwrap()
                .len(),
            1
        );
    }
}

#[test]
fn successful_installation_starts_or_restarts_only_when_requested() {
    for (inactive, no_start, expected) in [
        (
            "1",
            false,
            vec![
                "--user show --all --property=LoadState,ActiveState,UnitFileState,SubState,FragmentPath,NeedDaemonReload -- omega.service",
                "--user daemon-reload",
                "--user enable --now -- omega.service",
            ],
        ),
        (
            "0",
            false,
            vec![
                "--user show --all --property=LoadState,ActiveState,UnitFileState,SubState,FragmentPath,NeedDaemonReload -- omega.service",
                "--user daemon-reload",
                "--user enable --now -- omega.service",
                "--user restart -- omega.service",
            ],
        ),
        (
            "0",
            true,
            vec![
                "--user show --all --property=LoadState,ActiveState,UnitFileState,SubState,FragmentPath,NeedDaemonReload -- omega.service",
                "--user daemon-reload",
                "--user enable -- omega.service",
            ],
        ),
    ] {
        let machine = Machine::new();
        let mut command = machine.command("install");
        command.env("TEST_INACTIVE", inactive);
        if no_start {
            command.arg("--no-start");
        }
        let output = command.output().unwrap();
        let diagnostic = String::from_utf8_lossy(&output.stderr);
        assert!(output.status.success(), "{diagnostic}");
        assert_eq!(machine.events(), expected);
        assert!(
            diagnostic.contains(if no_start {
                "the daemon runs at the next login"
            } else {
                "the daemon is running, and runs at every login"
            }),
            "{diagnostic}"
        );
    }
}

#[test]
fn missing_systemctl_is_an_error_with_the_operation_and_diagnostic_command() {
    let machine = Machine::new();
    let output = machine
        .command("install")
        .env("PATH", machine.root.join("missing"))
        .output()
        .unwrap();
    let diagnostic = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success());
    assert!(
        diagnostic.contains("could not run systemctl --user show"),
        "{diagnostic}"
    );
    assert!(
        diagnostic.contains("systemctl --user status omega.service"),
        "{diagnostic}"
    );
    assert!(machine.events().is_empty());
}

#[test]
fn uninstall_preserves_the_unit_when_disable_fails_and_reports_reload_failure() {
    for operation in ["disable", "daemon-reload"] {
        let machine = Machine::new();
        AtomicFile::at(machine.unit())
            .write(b"installed unit")
            .unwrap();
        let output = machine
            .command("uninstall")
            .env("TEST_FAILURE", operation)
            .output()
            .unwrap();
        Machine::failure(&output, operation);
        if operation == "disable" {
            assert_eq!(std::fs::read(machine.unit()).unwrap(), b"installed unit");
            assert_eq!(machine.events(), ["--user disable --now -- omega.service"]);
        } else {
            assert!(!machine.unit().exists());
            assert_eq!(
                machine.events(),
                [
                    "--user disable --now -- omega.service",
                    "--user daemon-reload"
                ]
            );
        }
    }
}

#[test]
fn query_failures_are_not_inactive_and_prevent_installation() {
    let machine = Machine::new();
    let output = machine
        .command("install")
        .env("TEST_FAILURE", "show")
        .output()
        .unwrap();
    Machine::failure(&output, "show");
    assert!(!machine.unit().exists());
    assert_eq!(
        RecoveryStore::new(&machine.layout)
            .receipts()
            .unwrap()
            .len(),
        0
    );
    assert_eq!(machine.events().len(), 1);
}

#[test]
fn status_reports_failed_state_and_propagates_manager_errors() {
    let machine = Machine::new();
    let output = machine
        .command("status")
        .env("TEST_STATE", "failed")
        .output()
        .unwrap();
    assert!(output.status.success());
    let diagnostic = String::from_utf8_lossy(&output.stderr);
    assert!(diagnostic.contains("failed"), "{diagnostic}");
    assert!(diagnostic.contains("not installed"), "{diagnostic}");

    let output = machine
        .command("status")
        .env("TEST_FAILURE", "show")
        .output()
        .unwrap();
    Machine::failure(&output, "show");
}

#[test]
fn a_relative_service_directory_is_rejected_before_any_effect() {
    let machine = Machine::new();
    let output = machine
        .command("install")
        .env("OMEGA_SERVICE_DIR", "relative")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("directory must be absolute"));
    assert!(machine.events().is_empty());
    assert!(!machine.unit().exists());
}
