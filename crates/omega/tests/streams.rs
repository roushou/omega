//! Per-application audio streams read their topic and declare the audio capability.

use omega::testing::{Drawn, State, manifest_of};
use omega::{Command, Surface, View, ui::Text};
use omega_proto::omega::{AudioStream, AudioStreamsState, Capability};

#[derive(omega::Surface)]
struct Playing {
    streams: omega::platform::audio::Streams,
}

impl Surface for Playing {
    type Model = ();
    type Message = std::convert::Infallible;
    type Effects = ();
    fn update(
        &self,
        _: &mut (),
        message: Self::Message,
        _: &(),
    ) -> omega::surface::Task<Self::Message> {
        match message {}
    }
    fn render(&self, _: &(), _: &omega::surface::Events<Self::Message>) -> View {
        Text::new(self.streams.streams().len()).into()
    }
}

#[test]
fn streams_read_their_topic() {
    let drawn = Drawn::of::<Playing>(&State::new().with(AudioStreamsState {
        streams: vec![
            AudioStream {
                index: 1,
                app: "vlc".into(),
                volume: 0.5,
                muted: false,
            },
            AudioStream {
                index: 2,
                app: "chromium".into(),
                volume: 0.8,
                muted: true,
            },
        ],
    }))
    .unwrap();
    assert_eq!(drawn.text(), "2");
}

#[derive(omega::Command)]
struct Adjust {
    streams: omega::platform::audio::StreamControl,
    sink: omega::platform::audio::SinkControl,
    source: omega::platform::audio::SourceControl,
}
impl Command for Adjust {
    const ID: &'static str = "adjust";

    type Input = ();
    type Output = ();
    async fn call(&self, _: ()) -> omega::Result<()> {
        let _ = (&self.streams, &self.sink, &self.source);
        Ok(())
    }
}

#[test]
fn stream_and_device_controls_declare_the_audio_capability() {
    let manifest = manifest_of(&omega::Plugin::new("audio", "0.1.0").command::<Adjust>());
    let granted = manifest.granted().unwrap();
    assert!(granted.contains(&Capability::Audio));
}
