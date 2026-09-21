//! pactl volume and command conversion tests without a sound server.

mod common;

use omega_platform::pipewire::{PipeWire, Sinks, Sources, Streams};
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

/// Captured from `pactl --format=json list sources`, trimmed to what is read.
const SOURCES: &str = r#"[
  {
    "name": "mic",
    "mute": false,
    "volume": { "mono": { "value": 32768, "value_percent": "50%" } }
  }
]"#;

/// Captured from `pactl --format=json list sink-inputs`, trimmed to what is read.
const SINK_INPUTS: &str = r#"[
  {
    "index": 42,
    "mute": false,
    "application.name": "Chromium",
    "volume": { "front-left": { "value": 49152, "value_percent": "75%" } }
  },
  {
    "index": 43,
    "mute": true,
    "application.name": "vlc",
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

    // Clamp volume to the protocol range.
    let loud = PipeWire::arguments(&set_volume::Change::Absolute(4.0));
    assert_eq!(loud.last().unwrap(), "100%");
}

#[test]
fn muting_names_the_default_sink_like_everything_else() {
    let toggle = PipeWire::arguments(&set_volume::Change::ToggleMute(true));
    assert_eq!(toggle, vec!["set-sink-mute", "@DEFAULT_SINK@", "toggle"]);
}

#[test]
fn absolute_mute_and_unmute_do_not_toggle() {
    for (muted, argument) in [(true, "1"), (false, "0")] {
        let arguments = PipeWire::arguments(&set_volume::Change::Muted(muted));
        assert_eq!(arguments, ["set-sink-mute", "@DEFAULT_SINK@", argument]);
    }
}

#[test]
fn the_default_source_is_reported_with_its_mute_and_level() {
    let (volume, muted) = Sources::parse(SOURCES, "mic").unwrap();
    assert!((volume - 0.5).abs() < 0.01, "{volume}");
    assert!(!muted);
}

#[test]
fn a_machine_with_no_default_source_is_silent_and_muted() {
    let (volume, muted) = Sources::parse(SOURCES, "nothing-here").unwrap();
    assert_eq!(volume, 0.0);
    assert!(muted);
}

#[test]
fn streams_keep_their_index_app_level_and_mute() {
    let streams = Streams::parse(SINK_INPUTS).unwrap();
    assert_eq!(streams.len(), 2);
    assert_eq!(streams[0].index, 42);
    assert_eq!(streams[0].app, "Chromium");
    assert!((streams[0].volume - 0.75).abs() < 0.01);
    assert!(!streams[0].muted);
    assert_eq!(streams[1].index, 43);
    assert!(streams[1].muted);
}

// ---- against the machine this is running on ----

use omega_proto::omega::state_topic;

#[tokio::test]
#[ignore = "needs pactl and a running sound server; run with --ignored"]
async fn it_reads_the_machine_it_is_running_on() {
    let mut pipewire = PipeWire::new();

    let patch = common::first(&mut pipewire).await.expect("pactl answered");
    assert_eq!(patch.topics.len(), 2);
    assert_eq!(patch.topics[0].topic, "audio");
    assert_eq!(patch.topics[1].topic, "audio-streams");

    let Some(state_topic::Value::Audio(audio)) = patch.topics[0].value.as_ref() else {
        panic!("expected an audio reading");
    };
    assert!((0.0..=1.0).contains(&audio.volume), "{audio:?}");
    assert!(
        patch.topics[1]
            .value
            .as_ref()
            .is_some_and(|value| { matches!(value, state_topic::Value::AudioStreams(_)) }),
        "expected an audio-streams reading"
    );

    // Ignore subscription events unrelated to the selected audio device.
    assert!(
        common::waits(&mut pipewire).await,
        "a second reading should wait"
    );
}
