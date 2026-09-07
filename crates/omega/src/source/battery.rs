//! The battery.

use omega_wire::omega::BatteryState;

use crate::context::Context;
use crate::source::reads;
use crate::units::{Percent, Remaining};

/// The machine's battery, as a widget sees it.
///
/// ```no_run
/// # use omega::{Battery, Text, Ui, Widget};
/// #[derive(omega::Widget)]
/// struct Charge {
///     battery: Battery,
/// }
///
/// impl Widget for Charge {
///     fn render(&self) -> Ui {
///         Text::new(self.battery.charge()).into()
///     }
/// }
/// ```
#[derive(Debug)]
pub struct Battery {
    context: Context,
}

reads!(Battery, Battery, BatteryState);

impl Battery {
    /// How full it is. Prints itself as `80%`.
    pub fn charge(&self) -> Percent {
        self.read()
            .map(|battery| Percent::of(battery.level))
            .unwrap_or(Percent::ZERO)
    }

    pub fn is_charging(&self) -> bool {
        self.read().is_some_and(|battery| battery.charging)
    }

    /// How long until it is empty, or `None` while charging or unknown.
    pub fn until_empty(&self) -> Option<Remaining> {
        let battery = self.read()?;
        match battery.charging {
            true => None,
            false => Remaining::seconds(battery.seconds_to_empty),
        }
    }

    /// How long until it is full, or `None` while discharging or unknown.
    pub fn until_full(&self) -> Option<Remaining> {
        let battery = self.read()?;
        match battery.charging {
            true => Remaining::seconds(battery.seconds_to_full),
            false => None,
        }
    }

    /// Whichever of the two applies right now — what a bar actually shows.
    pub fn remaining(&self) -> Option<Remaining> {
        self.until_empty().or_else(|| self.until_full())
    }
}
