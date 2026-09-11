//! Turning Hyprland's monitors into the ontology.
//!
//! The answer is JSON, so the whole translation is testable with no
//! compositor in the room — which is the point of parsing being its own step.

mod common;

use omega_brokers::Hyprland;
use omega_brokers::hyprland::Monitors;
use omega_proto::omega::state_topic;

/// Captured from `hyprctl -j monitors`, trimmed to the fields read.
const LAPTOP: &str = r#"[{
    "id": 0,
    "name": "eDP-1",
    "description": "Lenovo Group Limited 0x403D",
    "width": 1920,
    "height": 1200,
    "refreshRate": 60.02600,
    "x": 0,
    "y": 0,
    "scale": 1,
    "focused": true,
    "dpmsStatus": true,
    "disabled": false
}]"#;

#[test]
fn a_monitor_becomes_the_ontologys_view_of_it() {
    let state = Monitors::parse(LAPTOP).expect("valid monitors");
    assert_eq!(state.monitors.len(), 1);

    let monitor = &state.monitors[0];
    assert_eq!(monitor.id, "eDP-1");
    assert!(monitor.connected);
    assert_eq!(monitor.width, 1920);
    assert_eq!(monitor.height, 1200);
    assert_eq!(monitor.x, 0);
    assert_eq!(monitor.scale, 1.0);
}

#[test]
fn a_refresh_rate_keeps_its_decimals() {
    // 60.026 Hz is a real mode, not a rounding of 60. The schema carries
    // millihertz so a bar reporting the mode can say which one it is.
    let state = Monitors::parse(LAPTOP).unwrap();
    assert_eq!(state.monitors[0].refresh_mhz, 60_026);
}

#[test]
fn the_focused_monitor_is_the_primary_one() {
    // Hyprland has no notion of a primary monitor. Focused is the nearest
    // true thing, and it is what a bar means when it asks.
    assert!(Monitors::parse(LAPTOP).unwrap().monitors[0].primary);
}

#[test]
fn a_disabled_monitor_is_not_connected() {
    let off = LAPTOP.replace("\"disabled\": false", "\"disabled\": true");
    assert!(!Monitors::parse(&off).unwrap().monitors[0].connected);
}

#[test]
fn a_field_hyprland_omits_takes_its_default() {
    // Older Hyprland does not report `disabled`. A monitor it is describing
    // at all is one it is driving, so absent reads as connected rather than
    // as a parse failure that blanks every display.
    let older = LAPTOP.replace(",\n    \"disabled\": false", "");
    assert!(Monitors::parse(&older).unwrap().monitors[0].connected);
}

#[test]
fn no_monitors_is_an_answer() {
    // A session with every display asleep says so, and that is a reading —
    // not a reason to fail and back off.
    assert!(Monitors::parse("[]").unwrap().monitors.is_empty());
}

// ---- against the machine this is running on ----

#[tokio::test]
#[ignore = "needs a Hyprland session; run with --ignored"]
async fn it_reads_the_session_it_is_running_in() {
    let mut hyprland = Hyprland::new();

    // A fresh connection reports everything it covers; after that, only what
    // the event that woke it can have changed.
    let patch = common::first(&mut hyprland)
        .await
        .expect("Hyprland answered");
    let topics: Vec<&str> = patch
        .topics
        .iter()
        .map(|topic| topic.topic.as_str())
        .collect();
    assert_eq!(topics, vec!["monitors", "workspaces", "window", "input"]);

    let Some(state_topic::Value::Monitors(display)) = patch.topics[0].value.as_ref() else {
        panic!("expected a display reading");
    };
    assert!(
        !display.monitors.is_empty(),
        "a session being drawn has a monitor"
    );

    let Some(state_topic::Value::Workspaces(workspaces)) = patch.topics[1].value.as_ref() else {
        panic!("expected a workspaces reading");
    };
    assert!(
        workspaces.workspaces.iter().any(|w| w.active),
        "one workspace is the one being looked at"
    );
}

// ---- workspaces and focus ----

use omega_brokers::hyprland::Session;

const WORKSPACES: &str = r#"[
  {"id": 3, "name": "3", "monitor": "eDP-1", "windows": 2},
  {"id": 1, "name": "1", "monitor": "eDP-1", "windows": 1}
]"#;

#[test]
fn workspaces_come_back_in_order_with_the_active_one_marked() {
    let state = Session::workspaces(WORKSPACES, "3").expect("valid workspaces");
    let ids: Vec<i32> = state.workspaces.iter().map(|w| w.id).collect();

    // Hyprland answers in whatever order it holds them. A bar shows them in
    // order, and sorting here is what stops every widget doing it.
    assert_eq!(ids, vec![1, 3]);
    assert!(!state.workspaces[0].active);
    assert!(state.workspaces[1].active);
    assert_eq!(state.workspaces[1].windows, 2);
    assert_eq!(state.workspaces[0].monitor_id, "eDP-1");
}

#[test]
fn the_active_workspace_is_read_by_name() {
    assert_eq!(
        Session::active_name(r#"{"id": 1, "name": "mail", "monitor": "eDP-1", "windows": 0}"#),
        "mail"
    );
}

#[test]
fn an_empty_workspace_has_nothing_focused() {
    // Hyprland answers `{}` when nothing has focus. That is nothing focused,
    // not a window with no name — a bar drawing an empty title would look
    // like a bug rather than an empty desktop.
    assert!(Session::window("{}").unwrap().focused.is_none());
}

#[test]
fn the_focused_window_carries_what_a_bar_shows() {
    let json = r#"{
        "class": "com.mitchellh.ghostty",
        "title": "0 · 1:nvim",
        "workspace": {"id": 1, "name": "1"},
        "pid": 4242,
        "floating": false,
        "fullscreen": 2
    }"#;
    let focused = Session::window(json).unwrap().focused.expect("a window");

    assert_eq!(focused.app_id, "com.mitchellh.ghostty");
    assert_eq!(focused.title, "0 · 1:nvim");
    assert_eq!(focused.workspace, "1");
    assert_eq!(focused.pid, 4242);
    assert!(!focused.floating);
    // Hyprland reports a mode, not a flag. Anything but zero is fullscreen.
    assert!(focused.fullscreen);
}

#[test]
fn the_keyboard_somebody_types_on_is_the_main_one() {
    // A laptop reports half a dozen keyboards — a power button, a video bus.
    // Reading the first would report the layout of a power button.
    let devices = r#"{"keyboards": [
        {"name": "power-button", "layout": "us", "active_keymap": "English (US)", "main": false},
        {"name": "at-translated-set-2", "layout": "fr", "active_keymap": "French", "main": true}
    ]}"#;
    let input = Session::input(devices).expect("valid devices");

    assert_eq!(input.keyboard, "at-translated-set-2");
    assert_eq!(input.layout, "fr");
    assert_eq!(input.keymap, "French");
}

#[test]
fn a_session_with_no_main_keyboard_is_a_reading() {
    let input = Session::input(r#"{"keyboards": []}"#).unwrap();
    assert_eq!(input.keyboard, "");
    assert_eq!(input.layout, "");
}

// ---- dispatching ----

use omega_brokers::hyprland::Dispatch;
use omega_proto::omega::{
    CloseWindow, Direction, MoveToMonitor, MoveToWorkspace, SwitchWorkspace, ToggleFloating,
    ToggleFullscreen, WindowSelector, action, move_to_workspace, switch_workspace, window_selector,
};

fn focused() -> Option<WindowSelector> {
    Some(WindowSelector {
        target: Some(window_selector::Target::Focused(true)),
    })
}

fn named(app_id: &str) -> Option<WindowSelector> {
    Some(WindowSelector {
        target: Some(window_selector::Target::AppId(app_id.into())),
    })
}

#[test]
fn a_workspace_is_switched_to_by_number_name_or_direction() {
    let switch = |target| {
        Dispatch::of(&action::Kind::SwitchWorkspace(SwitchWorkspace {
            target: Some(target),
        }))
    };

    assert_eq!(
        switch(switch_workspace::Target::Index(3)).unwrap(),
        "workspace 3"
    );
    assert_eq!(
        switch(switch_workspace::Target::Name("mail".into())).unwrap(),
        "workspace name:mail"
    );
    // Hyprland's relative form, which wraps within the monitor.
    assert_eq!(
        switch(switch_workspace::Target::Direction(Direction::Next as i32)).unwrap(),
        "workspace e+1"
    );
    assert_eq!(
        switch(switch_workspace::Target::Direction(
            Direction::Previous as i32
        ))
        .unwrap(),
        "workspace e-1"
    );
}

#[test]
fn the_focused_window_has_its_own_dispatcher() {
    // `closewindow activewindow` is not what Hyprland wants for the focused
    // one, and `killactive` is the form that works when no selector matches.
    let close = |window| Dispatch::of(&action::Kind::CloseWindow(CloseWindow { window }));

    assert_eq!(close(focused()).unwrap(), "killactive");
    assert_eq!(close(None).unwrap(), "killactive", "unset means focused");
    assert_eq!(
        close(named("firefox")).unwrap(),
        "closewindow class:firefox"
    );
}

#[test]
fn a_window_moves_with_the_workspace_it_is_sent_to() {
    let moved = Dispatch::of(&action::Kind::MoveToWorkspace(MoveToWorkspace {
        target: Some(move_to_workspace::Target::Index(2)),
        window: named("kitty"),
    }));
    assert_eq!(moved.unwrap(), "movetoworkspace 2,class:kitty");

    let monitor = Dispatch::of(&action::Kind::MoveToMonitor(MoveToMonitor {
        monitor_id: "eDP-1".into(),
        window: focused(),
    }));
    assert_eq!(monitor.unwrap(), "movewindow mon:eDP-1");
}

#[test]
fn fullscreen_is_refused_for_a_window_that_is_not_focused() {
    // Hyprland fullscreens the focused window and takes no selector. Doing it
    // to whichever window happens to be focused instead would be doing
    // something the caller did not ask for.
    let focused_one = Dispatch::of(&action::Kind::ToggleFullscreen(ToggleFullscreen {
        window: focused(),
    }));
    assert_eq!(focused_one.unwrap(), "fullscreen 1");

    let other = Dispatch::of(&action::Kind::ToggleFullscreen(ToggleFullscreen {
        window: named("firefox"),
    }));
    assert!(other.is_none(), "refused, not done to the wrong window");
}

#[test]
fn a_name_that_would_end_the_line_is_refused() {
    // A dispatch is one line on a socket. A newline in a window class would
    // make the rest of it a second dispatch, and a semicolon chains them —
    // so a name carrying either is not sent at all.
    for hostile in ["", "firefox\nkillactive", "firefox;killactive", "a\rb"] {
        let close = Dispatch::of(&action::Kind::ToggleFloating(ToggleFloating {
            window: named(hostile),
        }));
        assert!(close.is_none(), "{hostile:?} should not be dispatched");
    }

    assert_eq!(
        Dispatch::of(&action::Kind::ToggleFloating(ToggleFloating {
            window: named("kitty")
        }))
        .unwrap(),
        "togglefloating class:kitty"
    );
}

#[test]
fn workspace_dispatch_rejects_delimiters_and_monitor_moves_refuse_ignored_selectors() {
    for name in [
        "work;dispatch exec bad",
        "work\nother",
        "work,other",
        "work\0other",
    ] {
        assert!(
            Dispatch::of(&action::Kind::SwitchWorkspace(SwitchWorkspace {
                target: Some(switch_workspace::Target::Name(name.into())),
            }))
            .is_none()
        );
        assert!(
            Dispatch::of(&action::Kind::MoveToWorkspace(MoveToWorkspace {
                target: Some(move_to_workspace::Target::Name(name.into())),
                window: None,
            }))
            .is_none()
        );
    }
    assert!(
        Dispatch::of(&action::Kind::MoveToMonitor(MoveToMonitor {
            monitor_id: "DP-1".into(),
            window: named("kitty"),
        }))
        .is_none()
    );
    assert!(
        Dispatch::of(&action::Kind::SwitchWorkspace(SwitchWorkspace {
            target: Some(switch_workspace::Target::Direction(999)),
        }))
        .is_none()
    );
}
