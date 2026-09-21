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
    // Clipboard capture must write to stdout.
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
    // File captures require an explicit destination.
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
    // Quote paths passed through the screenshot-to-clipboard shell pipeline.
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
    assert_eq!(
        broker.actions(),
        &[
            ActionKind::Screenshot,
            ActionKind::CaptureText,
            ActionKind::RecordScreen
        ]
    );
    assert!(broker.topics().is_empty());
}

#[test]
fn ocr_commands_never_embed_a_hostile_region() {
    use omega_proto::omega::CaptureText;
    assert_eq!(
        Capture::text_command(&CaptureText::default()).unwrap(),
        "grim -g \"$(slurp)\" - | tesseract stdin stdout | wl-copy"
    );
    assert!(
        Capture::text_command(&CaptureText {
            region_monitor_id: "eDP-1;rm -rf ~".into(),
        })
        .is_none()
    );
}
