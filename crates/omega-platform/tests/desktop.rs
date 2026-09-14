use omega_platform::desktop::Capture;
use omega_platform::{Broker, Desktop};
use omega_proto::{ActionKind, omega::Screenshot};

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
fn it_claims_capturing_and_reports_nothing() {
    let broker = Desktop::new();
    assert_eq!(broker.actions(), &[ActionKind::Screenshot]);
    assert!(broker.topics().is_empty());
}
