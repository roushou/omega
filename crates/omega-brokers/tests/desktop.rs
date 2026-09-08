//! Launching an application, and taking a screenshot.

use omega_brokers::desktop::{Capture, DesktopEntry};
use omega_brokers::{Broker, Desktop};
use omega_proto::ActionKind;
use omega_proto::omega::Screenshot;

const ENTRY: &str = "[Desktop Entry]
Name=Firefox
Exec=firefox %u
Icon=firefox
Type=Application

[Desktop Action new-private-window]
Name=New Private Window
Exec=firefox --private-window %u";

#[test]
fn field_codes_are_not_arguments() {
    // The spec lets an Exec carry placeholders a launcher substitutes or
    // drops. Passing them through opens an editor with a file literally
    // named `%f`.
    assert_eq!(DesktopEntry::command(ENTRY).unwrap(), "firefox");
    assert_eq!(
        DesktopEntry::command("[Desktop Entry]\nExec=gimp %F %i %c").unwrap(),
        "gimp"
    );
}

#[test]
fn only_the_desktop_entry_group_is_read() {
    // An action group further down has its own Exec. Taking the first one
    // found could launch "new private window" instead of the browser.
    let actions_first = "[Desktop Action other]\nExec=wrong\n\n[Desktop Entry]\nExec=right";
    assert_eq!(DesktopEntry::command(actions_first), None);
    assert!(DesktopEntry::command(ENTRY).unwrap().starts_with("firefox"));
}

#[test]
fn an_entry_with_nothing_to_run_names_nothing() {
    assert_eq!(DesktopEntry::command("[Desktop Entry]\nName=Broken"), None);
    assert_eq!(DesktopEntry::command("[Desktop Entry]\nExec=%f"), None);
}

#[test]
fn a_desktop_id_is_looked_for_most_specific_first() {
    let paths = DesktopEntry::paths("firefox");
    let shown: Vec<String> = paths.iter().map(|p| p.display().to_string()).collect();

    assert!(
        shown
            .iter()
            .all(|p| p.ends_with("applications/firefox.desktop"))
    );
    // A user's own copy is looked for before the one a package installed, so
    // an override actually overrides.
    let user = shown.iter().position(|p| p.contains(".local/share"));
    let system = shown.iter().position(|p| p.contains("/usr/share"));
    if let (Some(user), Some(system)) = (user, system) {
        assert!(user < system, "{shown:?}");
    }

    // Given with or without the suffix, it means the same file.
    assert_eq!(paths, DesktopEntry::paths("firefox.desktop"));
}

fn shot(fullscreen: bool, monitor: &str, path: &str, clipboard: bool) -> Screenshot {
    Screenshot {
        fullscreen,
        region_monitor_id: monitor.into(),
        output_path: path.into(),
        clipboard,
    }
}

#[test]
fn a_screenshot_says_where_it_is_going() {
    assert_eq!(
        Capture::command(&shot(true, "", "/tmp/shot.png", false)).unwrap(),
        "grim /tmp/shot.png"
    );
    // grim writes to stdout when told to, which is what the clipboard wants.
    assert_eq!(
        Capture::command(&shot(true, "", "", true)).unwrap(),
        "grim - | wl-copy"
    );
    // Both means both, which grim cannot do in one pass.
    assert_eq!(
        Capture::command(&shot(true, "", "/tmp/s.png", true)).unwrap(),
        "grim /tmp/s.png && wl-copy < /tmp/s.png"
    );
}

#[test]
fn a_screenshot_with_nowhere_to_go_is_refused() {
    // grim's own default is a dated file in the working directory, which for
    // a daemon is not a place anybody will find it.
    assert!(Capture::command(&shot(true, "", "", false)).is_none());
}

#[test]
fn neither_a_monitor_nor_the_whole_screen_means_ask() {
    assert_eq!(
        Capture::command(&shot(false, "", "/tmp/s.png", false)).unwrap(),
        "grim -g \"$(slurp)\" /tmp/s.png"
    );
    assert_eq!(
        Capture::command(&shot(false, "eDP-1", "/tmp/s.png", false)).unwrap(),
        "grim -o eDP-1 /tmp/s.png"
    );
}

#[test]
fn a_path_that_would_start_a_second_command_is_refused() {
    // These go through a shell — `grim … | wl-copy` is a pipeline — so a path
    // carrying a semicolon or a backtick would be a command of its own.
    for hostile in ["/tmp/a;rm -rf ~", "/tmp/`id`", "/tmp/a|sh", "/tmp/a$(id)"] {
        assert!(
            Capture::command(&shot(true, "", hostile, false)).is_none(),
            "{hostile:?} should not be run"
        );
    }
}

#[test]
fn it_claims_launching_and_capturing_and_reports_nothing() {
    let broker = Desktop::new();
    assert_eq!(
        broker.actions(),
        &[ActionKind::LaunchApp, ActionKind::Screenshot]
    );
    assert!(broker.topics().is_empty());
}
