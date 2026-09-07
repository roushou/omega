//! The service that makes the daemon part of a session.
//!
//! The unit file cannot be carried in the binary the way the renderer is: it
//! names the binary's own path. So what these check is the consequence — that
//! an installed service is readable back, and that a service running a
//! *different* omega is told apart from one running this one, because on a
//! machine with two of them that is the difference between a desktop that
//! came back after a reboot and one that only looks like it did.

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

    // The whole reason the unit file is generated rather than carried: it has
    // to name a binary, and which binary is the question a person installing
    // a service is answering.
    assert!(
        unit.contains("ExecStart=/usr/local/bin/omega daemon"),
        "{unit}"
    );

    // It belongs to the session on both sides: the daemon binds sockets in
    // $XDG_RUNTIME_DIR and draws through a shell that has one, so a daemon
    // outliving the session has nothing left to serve.
    assert!(unit.contains("PartOf=graphical-session.target"), "{unit}");
    assert!(unit.contains("WantedBy=graphical-session.target"), "{unit}");
}

#[test]
fn systemd_outwaits_the_daemons_own_shutdown() {
    let unit = Service::unit(&omega());

    // The daemon gives each unit five seconds to stop. A `TimeoutStopSec`
    // under that is systemd killing the daemon in the middle of stopping
    // them, every time, and it would look like a crash on shutdown.
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

    Service::install(&path, &omega()).unwrap();
    assert_eq!(Service::installed(&path, &omega()), Installed::Current);
}

#[test]
fn a_service_running_another_omega_says_which() {
    let path = unit_path("other");
    Service::install(&path, Path::new("/home/someone/.cargo/bin/omega")).unwrap();

    // The failure that looks like every other failure: the service came back
    // after the reboot, and came back as somebody else.
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
    Service::install(&path, &omega()).unwrap();

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
    Service::install(&path, &omega()).unwrap();

    assert!(Service::uninstall(&path).unwrap());
    assert_eq!(Service::installed(&path, &omega()), Installed::Missing);

    // Removing what is not there is not a failure, and says so.
    assert!(!Service::uninstall(&path).unwrap());
}

#[test]
fn a_binary_in_a_build_directory_is_not_somewhere_to_point_a_service() {
    // A service is a promise to run this again after a reboot, and cargo's
    // build directory is not a promise anybody made.
    let target = std::env::temp_dir().join("omega-service-buildish/target");
    std::fs::create_dir_all(target.join("release")).unwrap();
    std::fs::write(target.join("CACHEDIR.TAG"), "Signature: 8a477f597d28d172").unwrap();

    assert!(Service::is_a_build_artifact(&target.join("release/omega")));
    assert!(!Service::is_a_build_artifact(&omega()));
}
