//! Generated service paths, session binding, and executable mismatch detection.

use std::path::{Path, PathBuf};

use omega_cli::service::{Installed, Service};

/// A unit file of its own, so a test never writes where systemd reads — and
/// never has to reach for a process-wide variable that its neighbours share.
fn unit_path(label: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("omega-service-{label}-{nanos}"));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(Service::NAME)
}

fn omega() -> PathBuf {
    PathBuf::from("/usr/local/bin/omega")
}

#[test]
fn a_service_runs_the_omega_that_installed_it() {
    let unit = Service::unit(&omega());

    // Generated service files must reference the installing executable.
    assert!(
        unit.contains("ExecStart=/usr/local/bin/omega daemon"),
        "{unit}"
    );

    // The service lifetime must match the user session.
    assert!(unit.contains("PartOf=graphical-session.target"), "{unit}");
    assert!(unit.contains("WantedBy=graphical-session.target"), "{unit}");
}

#[test]
fn systemd_outwaits_the_daemons_own_shutdown() {
    let unit = Service::unit(&omega());

    // The service stop timeout must exceed plugin shutdown grace.
    let timeout: u64 = unit
        .lines()
        .find_map(|line| line.trim().strip_prefix("TimeoutStopSec="))
        .expect("the unit says how long to wait")
        .parse()
        .expect("...as a number of seconds");
    assert!(timeout > 5, "{timeout} is not longer than the daemon's own");
}

#[test]
fn a_fresh_install_is_current() {
    let path = unit_path("fresh");

    assert_eq!(
        Service::installed(&path, &omega()),
        Installed::Missing,
        "nothing is installed until something installs it"
    );

    TestInstallation::install(&path, &omega()).unwrap();
    assert_eq!(Service::installed(&path, &omega()), Installed::Current);
}

#[test]
fn a_service_running_another_omega_says_which() {
    let path = unit_path("other");
    TestInstallation::install(&path, Path::new("/home/someone/.cargo/bin/omega")).unwrap();

    // Detect a service installed from a different executable.
    assert_eq!(
        Service::installed(&path, &omega()),
        Installed::Stale {
            program: Some("/home/someone/.cargo/bin/omega".to_string())
        }
    );
}

#[test]
fn a_hand_edited_unit_is_not_mistaken_for_this_one() {
    let path = unit_path("edited");
    TestInstallation::install(&path, &omega()).unwrap();

    let edited = std::fs::read_to_string(&path)
        .unwrap()
        .replace("Restart=on-failure", "Restart=no");
    std::fs::write(&path, edited).unwrap();

    // It still runs the right binary, so the program it reports is this one —
    // and it is still not what this omega would have written.
    assert_eq!(
        Service::installed(&path, &omega()),
        Installed::Stale {
            program: Some("/usr/local/bin/omega".to_string())
        }
    );
}

#[test]
fn uninstall_takes_away_what_was_installed() {
    let path = unit_path("gone");
    TestInstallation::install(&path, &omega()).unwrap();

    assert!(Service::uninstall(&path).unwrap());
    assert_eq!(Service::installed(&path, &omega()), Installed::Missing);

    // Removing what is not there is not a failure, and says so.
    assert!(!Service::uninstall(&path).unwrap());
}

#[test]
fn a_binary_in_a_build_directory_is_not_somewhere_to_point_a_service() {
    // Service installation requires a durable executable outside Cargo build output.
    let target = std::env::temp_dir().join("omega-service-buildish/target");
    std::fs::create_dir_all(target.join("release")).unwrap();
    std::fs::write(target.join("CACHEDIR.TAG"), "Signature: 8a477f597d28d172").unwrap();

    assert!(Service::is_a_build_artifact(&target.join("release/omega")));
    assert!(!Service::is_a_build_artifact(&omega()));
}

struct TestInstallation;

impl TestInstallation {
    fn install(
        path: &Path,
        program: &Path,
    ) -> anyhow::Result<omega_host::recovery::InstalledReplacement> {
        let root = path.parent().unwrap();
        let layout =
            omega_host::Layout::at(root.join("config"), root.join("state"), root.join("cache"));
        Service::install(
            path,
            program,
            &omega_host::recovery::RecoveryStore::new(&layout),
        )
    }
}
