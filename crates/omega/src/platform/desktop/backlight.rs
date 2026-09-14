//! The screen's brightness.

use crate::units::Percent;

use omega_proto::omega::{SetBacklight, action, set_backlight};

use crate::runtime::context::Context;

use crate::wiring::does;

crate::wiring::reading! {
    /// The screen's brightness.
    Backlight: omega_proto::omega::BacklightState
}

/// The screen's backlight, as a widget sees it.
///
/// A machine with no backlight — a desktop, or a monitor driven over DDC/CI
/// rather than sysfs — has no reading, which is what
/// [`has_reading`](Backlight::has_reading) answers.
///
/// ```no_run
/// # use omega::platform::desktop::Backlight;
/// # use omega::ui::Text;
/// # use omega::{View, Surface};
/// #[derive(omega::Surface)]
/// struct Brightness {
///     backlight: Backlight,
/// }
///
/// impl Surface for Brightness {
///     fn render(&self) -> View {
///         Text::new(self.backlight.level()).into()
///     }
/// }
/// ```
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

/// Permission to change the screen's brightness.
#[derive(Debug)]
pub struct Brightness {
    context: Context,
}

does!(Brightness, Backlight);

impl Brightness {
    /// Set it outright.
    pub fn set(&self, level: Percent) -> crate::effect::Effect {
        self.change(set_backlight::Change::AbsolutePercent(u32::from(
            level.whole_percent(),
        )))
    }

    /// Move it by whole percentage points: `5` is five percent brighter,
    /// `-5` dimmer. Clamped at both ends by the broker, so stepping down from
    /// 2% lands on 0 rather than wrapping.
    pub fn step(&self, delta: i32) -> crate::effect::Effect {
        self.change(set_backlight::Change::DeltaPercent(delta))
    }

    fn change(&self, change: set_backlight::Change) -> crate::effect::Effect {
        self.act(action::Kind::SetBacklight(SetBacklight {
            change: Some(change),
        }))
    }
}
