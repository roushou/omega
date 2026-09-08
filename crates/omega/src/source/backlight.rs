//! The screen's brightness.

use omega_proto::omega::BacklightState;

use crate::context::Context;
use crate::source::reads;
use crate::units::Percent;

/// The screen's backlight, as a widget sees it.
///
/// A machine with no backlight — a desktop, or a monitor driven over DDC/CI
/// rather than sysfs — has no reading, which is what
/// [`has_reading`](Backlight::has_reading) answers.
///
/// ```no_run
/// # use omega::{Backlight, Text, Ui, Widget};
/// #[derive(omega::Widget)]
/// struct Brightness {
///     backlight: Backlight,
/// }
///
/// impl Widget for Brightness {
///     fn render(&self) -> Ui {
///         Text::new(self.backlight.level()).into()
///     }
/// }
/// ```
#[derive(Debug)]
pub struct Backlight {
    context: Context,
}

reads!(Backlight, BacklightState);

impl Backlight {
    /// How bright it is. Prints itself as `60%`.
    pub fn level(&self) -> Percent {
        self.read()
            // The wire is a u32 because protobuf has no smaller integer; a
            // percentage above a hundred is a broker with a bug, not a
            // brighter screen.
            .map(|backlight| Percent::whole(backlight.percent.min(100) as u8))
            .unwrap_or(Percent::ZERO)
    }
}
