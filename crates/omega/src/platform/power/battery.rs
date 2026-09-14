//! System battery state.

crate::wiring::reading! {
    /// System battery state.
    Battery: omega_proto::omega::BatteryState
}

use crate::units::{Percent, Remaining};

/// Read system battery charge and charging state.
///
/// ```no_run
/// # use omega::platform::power::Battery;
/// # use omega::ui::Text;
/// # use omega::{View, Surface};
/// #[derive(omega::Surface)]
/// struct Charge {
///     battery: Battery,
/// }
///
/// impl Surface for Charge {
///     type Model = ();
///     type Message = std::convert::Infallible;
///     type Effects = ();
///     fn update(&self, _: &mut (), message: Self::Message, _: &()) -> omega::surface::Task<Self::Message> {
///         match message {}
///     }
///
///     fn render(&self, _: &(), _: &omega::surface::Events<Self::Message>) -> View {
///         Text::new(self.battery.charge()).into()
///     }
/// }
/// ```
impl Battery {
    /// Battery charge as a percentage.
    pub fn charge(&self) -> Percent {
        self.read()
            .map(|battery| Percent::of(battery.level))
            .unwrap_or(Percent::ZERO)
    }

    pub fn is_charging(&self) -> bool {
        self.read().is_some_and(|battery| battery.charging)
    }

    /// Estimated time until empty, or `None` while charging or when unknown.
    pub fn until_empty(&self) -> Option<Remaining> {
        let battery = self.read()?;
        if battery.charging {
            None
        } else {
            Remaining::seconds(battery.seconds_to_empty)
        }
    }

    /// Estimated time until full, or `None` while discharging or when unknown.
    pub fn until_full(&self) -> Option<Remaining> {
        let battery = self.read()?;
        if battery.charging {
            Remaining::seconds(battery.seconds_to_full)
        } else {
            None
        }
    }

    /// Estimated time until full while charging, or until empty otherwise.
    pub fn remaining(&self) -> Option<Remaining> {
        self.until_empty().or_else(|| self.until_full())
    }
}
