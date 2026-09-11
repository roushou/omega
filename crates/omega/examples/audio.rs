//! Output volume and mute controls. Run as a unit or place `indicator` and `panel`.
use omega::effect::Volume;
use omega::reading::Audio;
use omega::ui::{Button, Column, Size, Slider, Text};
use omega::{Args, Command, Percent, Plugin, Ui, Widget};

pub const UNIT: &str = env!("CARGO_PKG_NAME");

#[derive(omega::Widget, Debug)]
pub struct Indicator {
    audio: Audio,
}
impl Widget for Indicator {
    fn render(&self) -> Ui {
        if !self.audio.has_reading() {
            return Text::new("Audio unavailable").dim().into();
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
        Column::new()
            .gap(12)
            .child(Text::new("Audio").size(Size::Title).bold())
            .child(Text::new(format!("{}", self.audio.volume())).size(Size::Display))
            .child(
                Text::new(if self.audio.is_muted() {
                    "Output muted"
                } else {
                    "Output volume"
                })
                .dim(),
            )
            .child(
                Slider::new(self.audio.volume())
                    .on_change("volume")
                    .key("volume"),
            )
            .child(
                Button::new(if self.audio.is_muted() {
                    "Unmute"
                } else {
                    "Mute"
                })
                .fill()
                .on_press("mute")
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
    type Output = ();
    async fn call(&self, args: Args) -> omega::Result<()> {
        let value = args
            .get::<f64>(0)
            .filter(|v| v.is_finite() && (0.0..=1.0).contains(v))
            .ok_or_else(|| omega::Error::invalid("volume must be a fraction between 0 and 1"))?;
        self.volume.set(Percent::of(value)).await
    }
}

#[derive(omega::Command, Debug)]
pub struct Mute {
    volume: Volume,
}
impl Command for Mute {
    type Output = ();
    async fn call(&self, _: Args) -> omega::Result<()> {
        self.volume.toggle_mute().await
    }
}

pub fn plugin() -> Plugin {
    Plugin::named(UNIT, env!("CARGO_PKG_VERSION"))
        .widget_as::<Indicator>("indicator")
        .widget_as::<Panel>("panel")
        .command::<SetVolume>("volume")
        .command::<Mute>("mute")
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
            let called = Called::of::<SetVolume>(&State::new(), vec![value.into_value()]).await;
            assert!(called.answer.is_err());
            assert!(called.effects.is_empty());
        }
        let called = Called::of::<SetVolume>(&State::new(), vec![0.4.into_value()]).await;
        assert!(called.answer.is_ok());
        assert_eq!(called.effects.len(), 1);
    }
}
