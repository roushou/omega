//! Reading and setting the volume through `pactl`.
//!
//! PulseAudio's scale and the ontology's are not the same, and neither are
//! their spellings of a change. Both conversions are pure, so both are here
//! without a sound server in the room.

use omega_brokers::pipewire::{PipeWire, Sinks};
use omega_proto::omega::set_volume;

/// Captured from `pactl --format=json list sinks`, trimmed to what is read.
const SINKS: &str = r#"[
  {
    "name": "speakers",
    "mute": false,
    "volume": {
      "front-left":  { "value": 51123, "value_percent": "78%" },
      "front-right": { "value": 51123, "value_percent": "78%" }
    }
  },
  {
    "name": "headphones",
    "mute": true,
    "volume": { "mono": { "value": 65536, "value_percent": "100%" } }
  }
]"#;

#[test]
fn the_default_sink_is_the_one_reported() {
    let state = Sinks::parse(SINKS, "headphones").expect("valid sinks");

    assert_eq!(state.default_sink, "headphones");
    assert!(state.muted);
    assert_eq!(state.volume, 1.0);
}

#[test]
fn a_raw_pulseaudio_volume_becomes_a_fraction() {
    // 65536 is unattenuated, so 51123 is a bit over three quarters. A broker
    // passing the raw number through would report a volume of fifty thousand.
    let state = Sinks::parse(SINKS, "speakers").unwrap();
    assert!((state.volume - 0.78).abs() < 0.01, "{}", state.volume);
}

#[test]
fn a_machine_with_no_default_sink_is_silent_rather_than_absent() {
    // PipeWire is answering, and "nothing is playing anywhere" is a reading.
    let state = Sinks::parse(SINKS, "nothing-here").unwrap();
    assert_eq!(state.volume, 0.0);
    assert!(state.muted);
}

#[test]
fn a_positive_step_has_to_say_it_is_positive() {
    // `pactl set-sink-volume 5%` sets the volume *to* five. Dropping the sign
    // would turn every volume-up key into a mute.
    let up = PipeWire::arguments(&set_volume::Change::Delta(0.05));
    assert_eq!(up.last().unwrap(), "+5%");

    let down = PipeWire::arguments(&set_volume::Change::Delta(-0.05));
    assert_eq!(down.last().unwrap(), "-5%");
}

#[test]
fn an_absolute_level_is_a_percentage_of_unattenuated() {
    let set = PipeWire::arguments(&set_volume::Change::Absolute(0.42));
    assert_eq!(set.last().unwrap(), "42%");

    // The ontology's range is zero to one, and a caller outside it gets the
    // nearest end rather than an amplifier.
    let loud = PipeWire::arguments(&set_volume::Change::Absolute(4.0));
    assert_eq!(loud.last().unwrap(), "100%");
}

#[test]
fn muting_names_the_default_sink_like_everything_else() {
    let toggle = PipeWire::arguments(&set_volume::Change::ToggleMute(true));
    assert_eq!(toggle, vec!["set-sink-mute", "@DEFAULT_SINK@", "toggle"]);
}

// ---- against the machine this is running on ----

use omega_brokers::Broker;
use omega_proto::omega::state_topic;
use std::time::Duration;

#[tokio::test]
#[ignore = "needs pactl and a running sound server; run with --ignored"]
async fn it_reads_the_machine_it_is_running_on() {
    let mut pipewire = PipeWire::new();

    let patch = pipewire.next().await.expect("pactl answered");
    assert_eq!(patch.topics[0].topic, "audio");

    let Some(state_topic::Value::Audio(audio)) = patch.topics[0].value.as_ref() else {
        panic!("expected an audio reading");
    };
    assert!((0.0..=1.0).contains(&audio.volume), "{audio:?}");

    // The second reading waits on the subscription. `pactl subscribe` reports
    // every stream a browser opens, so a broker that woke on all of them
    // would run two processes per notification sound.
    let again = tokio::time::timeout(Duration::from_millis(500), pipewire.next()).await;
    assert!(again.is_err(), "a second reading should wait for an event");
}
