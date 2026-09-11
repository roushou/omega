//! Output volume and mute controls. Run as a unit or place `indicator` and `panel`.
use omega::effect::Volume;
use omega::reading::Audio;
use omega::ui::{Button, Metric, Section, Slider, Text};
use omega::{Command, Percent, Plugin, Ui, Widget};

pub const UNIT: &str = env!("CARGO_PKG_NAME");

#[derive(omega::Widget, Debug)]
pub struct Indicator {
    audio: Audio,
}
impl Widget for Indicator {
    fn render(&self) -> Ui {
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

#[derive(omega::Widget, Debug)]
pub struct Panel {
    audio: Audio,
}
impl Widget for Panel {
    fn render(&self) -> Ui {
        if !self.audio.has_reading() {
            return Text::new("Audio unavailable").into();
        }
        Section::new("Audio")
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
#[omega(name = "volume")]
pub struct SetVolume {
    volume: Volume,
}
impl Command for SetVolume {
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
    type Input = ();
    type Output = ();
    async fn call(&self, _: ()) -> omega::Result<()> {
        self.volume.toggle_mute().await
    }
}

pub fn plugin() -> Plugin {
    Plugin::named(UNIT, env!("CARGO_PKG_VERSION"))
        .widget_as::<Indicator>("indicator")
        .widget_as::<Panel>("panel")
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
        let state = State::new().absent(omega::reading::SystemTopic::Audio);
        assert_eq!(Drawn::of::<Panel>(&state).text(), "Audio unavailable");
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
