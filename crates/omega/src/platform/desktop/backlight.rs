//! Display backlight state and brightness control.

use crate::units::Percent;

use omega_proto::omega::{SetBacklight, action, set_backlight};

use crate::runtime::context::Context;

use crate::wiring::does;

crate::wiring::reading! {
    /// Display backlight brightness.
    Backlight: omega_proto::omega::BacklightState
}

/// Read sysfs display backlight brightness.
/// `has_reading()` is false when no supported backlight exists. External DDC/CI
/// monitor control is not supported.
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
///     type Model = ();
///     type Message = std::convert::Infallible;
///     type Effects = ();
///     fn update(&self, _: &mut (), message: Self::Message, _: &()) -> omega::surface::Task<Self::Message> {
///         match message {}
///     }
///
///     fn render(&self, _: &(), _: &omega::surface::Events<Self::Message>) -> View {
///         Text::new(self.backlight.level()).into()
///     }
/// }
/// ```
impl Backlight {
    /// Current brightness as a percentage.
    pub fn level(&self) -> Percent {
        self.read()
            // Clamp the protocol percentage to the supported 0–100 range.
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
    /// Set the display brightness.
    pub fn set(&self, level: Percent) -> crate::effect::Effect {
        self.change(set_backlight::Change::AbsolutePercent(u32::from(
            level.whole_percent(),
        )))
    }

    /// Adjust brightness by whole percentage points. Positive values increase
    /// brightness; negative values decrease it. The result is clamped to 0–100%.
    pub fn step(&self, delta: i32) -> crate::effect::Effect {
        self.change(set_backlight::Change::DeltaPercent(delta))
    }

    fn change(&self, change: set_backlight::Change) -> crate::effect::Effect {
        self.act(action::Kind::SetBacklight(SetBacklight {
            change: Some(change),
        }))
    }
}
