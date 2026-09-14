//! Configurable battery indicator.
//! Use `omega dev <name>` for live development and `cargo test` for fixture tests.
//! The system crate can depend on this library to configure its settings and placement.

use omega::platform::power::Battery;
use omega::surface::{Events, Task};
use omega::ui::{Row, Text};
use omega::{Plugin, Surface, View};
use std::convert::Infallible;

/// This plugin's name, for the config plane to refer to it by.
pub const UNIT: &str = env!("CARGO_PKG_NAME");

/// What the document can configure an instance with.
#[derive(omega::Config, Debug, Clone, PartialEq)]
pub struct Settings {
    /// Below this, and going down, the charge is drawn as urgent.
    pub low: u8,
}

impl Default for Settings {
    fn default() -> Self {
        Self { low: 20 }
    }
}

#[derive(omega::Surface)]
pub struct BatteryWidget {
    battery: Battery,
    #[omega(config)]
    settings: Settings,
}

impl Surface for BatteryWidget {
    type Model = ();
    type Message = Infallible;
    type Effects = ();
    fn render(&self, _: &(), _: &Events<Infallible>) -> View {
        // Missing battery readings must not display as zero charge.
        if !self.battery.has_reading() {
            return View::empty();
        }

        let charge = self.battery.charge();
        // Charging out of a low charge is not urgent, it is over — and red
        // for the twenty minutes it takes to stop being low says otherwise.
        let low = charge < self.settings.low && !self.battery.is_charging();

        let label = if low {
            Text::new(charge).warning().bold()
        } else {
            Text::new(charge).bold()
        };

        match self.battery.remaining() {
            Some(left) => Row::new()
                .gap(6)
                .child(label)
                .child(Text::new(left).muted())
                .into(),
            None => label.into(),
        }
    }

    fn update(&self, _: &mut (), message: Infallible, _: &()) -> Task<Infallible> {
        match message {}
    }
}

/// Everything this plugin offers. `main` runs it; a test can inspect it.
pub fn plugin() -> Plugin {
    omega::plugin!().surface(BatteryWidget)
}

#[cfg(test)]
mod tests {
    use super::*;
    use omega::config::Fields;
    use omega::testing::{Drawn, State};

    #[test]
    fn it_shows_the_charge() {
        let drawn = Drawn::of::<BatteryWidget>(&State::new().battery(0.8, false)).unwrap();
        assert_eq!(drawn.text(), "80%");
    }

    #[test]
    fn a_low_charge_is_drawn_as_urgent() {
        let state = State::new().battery(0.1, false);

        // The settings the document would hand this instance, written from
        // the same type the config plane writes them with.
        let strict = Settings { low: 20 }.write();

        let drawn = Drawn::configured::<BatteryWidget>(&state, &strict).unwrap();
        let figure = drawn.first("text").expect("the widget draws the charge");
        assert_eq!(drawn.prop(&figure, "tone").as_deref(), Some("warning"));
    }

    #[test]
    fn a_low_charge_on_the_wall_is_not_urgent() {
        let drawn = Drawn::of::<BatteryWidget>(&State::new().battery(0.1, true)).unwrap();
        let figure = drawn.first("text").expect("the widget draws the charge");
        assert_eq!(drawn.prop(&figure, "tone"), None);
    }

    #[test]
    fn a_machine_with_no_battery_draws_nothing() {
        let state = State::new().absent(omega::testing::SystemTopic::Battery);
        assert!(Drawn::of::<BatteryWidget>(&state).unwrap().is_empty());
    }

    #[test]
    fn it_declares_only_what_it_holds() {
        let manifest = omega::testing::manifest_of(&plugin());

        // Reading the battery is the only thing this plugin can do, and the
        // only thing it asked for.
        assert_eq!(manifest.state_topics, vec!["battery"]);
        assert_eq!(
            manifest.granted().unwrap(),
            vec![omega::internal::Capability::StateRead]
        );
    }
}
