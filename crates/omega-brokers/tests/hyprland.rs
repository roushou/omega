//! Turning Hyprland's monitors into the ontology.
//!
//! The answer is JSON, so the whole translation is testable with no
//! compositor in the room — which is the point of parsing being its own step.

use std::time::Duration;

use omega_brokers::hyprland::Monitors;
use omega_brokers::{Broker, Hyprland};
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

    let patch = hyprland.next().await.expect("Hyprland answered");
    assert_eq!(patch.topics[0].topic, "display");

    let Some(state_topic::Value::Display(display)) = patch.topics[0].value.as_ref() else {
        panic!("expected a display reading");
    };
    assert!(
        !display.monitors.is_empty(),
        "a session being drawn has a monitor"
    );

    // The second reading waits on the event socket. Hyprland streams every
    // window focus down it, so a broker that woke on all of them would re-read
    // the monitors on each keystroke.
    let again = tokio::time::timeout(Duration::from_millis(500), hyprland.next()).await;
    assert!(again.is_err(), "a second reading should wait for an event");
}
