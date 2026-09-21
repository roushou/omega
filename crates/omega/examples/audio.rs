//! Output volume and mute controls. Run as a plugin or place `indicator` and `panel`.
use omega::platform::audio::{Audio, Volume};
use omega::ui::{Button, Metric, Section, Slider, Text};
use omega::{Command, Percent, Plugin, Surface, Ui};

pub const PLUGIN: &str = env!("CARGO_PKG_NAME");

#[derive(omega::Surface, Debug)]
pub struct Indicator {
    audio: Audio,
}

impl Surface for Indicator {
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
    fn render(&self, _: &(), _: &omega::surface::Events<Self::Message>) -> Ui {
        if !self.audio.has_reading() {
            return Text::new("Audio unavailable").muted().into();
        }
        Text::new(if self.audio.is_muted() {
            "Muted".to_string()
        } else {
            format!("Vol {}", self.audio.volume())
        })
        .into()
    }
}

#[derive(omega::Surface, Debug)]
pub struct Panel {
    audio: Audio,
}

impl Surface for Panel {
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
    fn render(&self, _: &(), _: &omega::surface::Events<Self::Message>) -> Ui {
        if !self.audio.has_reading() {
            return Text::new("Audio unavailable").into();
        }
        Section::new()
            .title("Audio")
            .child(
                Metric::new(self.audio.volume()).label(if self.audio.is_muted() {
                    "Output muted"
                } else {
                    "Output volume"
                }),
            )
            .child(
                Slider::new(self.audio.volume())
                    .on_change(SetVolume)
                    .key("volume"),
            )
            .child(
                Button::new(if self.audio.is_muted() {
                    "Unmute"
                } else {
                    "Mute"
                })
                .fill_width()
                .secondary()
                .on_press(Mute)
                .key("mute"),
            )
            .into()
    }
}

#[derive(omega::Command, Debug)]
pub struct SetVolume {
    volume: Volume,
}

impl Command for SetVolume {
    const ID: &'static str = "volume";

    type Input = Percent;
    type Output = ();
    async fn call(&self, value: Percent) -> omega::Result<()> {
        self.volume.set(value).await
    }
}

#[derive(omega::Command, Debug)]
pub struct Mute {
    volume: Volume,
}

impl Command for Mute {
    const ID: &'static str = "mute";

    type Input = ();
    type Output = ();
    async fn call(&self, _: ()) -> omega::Result<()> {
        self.volume.toggle_mute().await
    }
}

pub fn plugin() -> Plugin {
    Plugin::new(PLUGIN, env!("CARGO_PKG_VERSION"))
        .surface_as::<Indicator>("indicator")
        .surface_as::<Panel>("panel")
        .command::<SetVolume>()
        .command::<Mute>()
}

fn main() -> omega::Result<()> {
    plugin().run()
}

#[cfg(test)]
mod tests {
    use super::*;
    use omega::config::IntoValue;
    use omega::testing::{Called, Drawn, State};
    #[test]
    fn absent_audio_is_not_zero_volume() {
        let state = State::new().absent(omega::testing::SystemTopic::Audio);
        assert_eq!(
            Drawn::of::<Panel>(&state).unwrap().text(),
            "Audio unavailable"
        );
    }
    #[tokio::test]
    async fn invalid_volume_does_not_act() {
        for value in [-1.0, 1.1, f64::NAN] {
            let called = Called::raw::<SetVolume>(&State::new(), vec![value.into_value()]).await;
            assert!(called.answer.is_err());
            assert!(called.effects.is_empty());
        }
        let called = Called::of::<SetVolume>(&State::new(), Percent::whole(40)).await;
        assert!(called.answer.is_ok());
        assert_eq!(called.effects.len(), 1);
    }
}
