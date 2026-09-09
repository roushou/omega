//! The battery.

use crate::reading::Battery;
use crate::units::{Percent, Remaining};

/// The machine's battery, as a widget sees it.
///
/// ```no_run
/// # use omega::reading::Battery;
/// # use omega::ui::Text;
/// # use omega::{Ui, Widget};
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
        if battery.charging {
            None
        } else {
            Remaining::seconds(battery.seconds_to_empty)
        }
    }

    /// How long until it is full, or `None` while discharging or unknown.
    pub fn until_full(&self) -> Option<Remaining> {
        let battery = self.read()?;
        if battery.charging {
            Remaining::seconds(battery.seconds_to_full)
        } else {
            None
        }
    }

    /// Whichever of the two applies right now — what a bar actually shows.
    pub fn remaining(&self) -> Option<Remaining> {
        self.until_empty().or_else(|| self.until_full())
    }
}
