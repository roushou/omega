//! Changing the screen's brightness. To *read* it, hold a `Backlight`.

use omega_proto::omega::{SetBacklight, action, set_backlight};

use crate::context::Context;
use crate::effect::does;
use crate::units::Percent;

/// Permission to change the screen's brightness.
#[derive(Debug)]
pub struct Brightness {
    context: Context,
}

does!(Brightness, Backlight);

impl Brightness {
    /// Set it outright.
    pub fn set(&self, level: Percent) {
        self.change(set_backlight::Change::AbsolutePercent(u32::from(
            level.whole_percent(),
        )));
    }

    /// Move it by whole percentage points: `5` is five percent brighter,
    /// `-5` dimmer. Clamped at both ends by the broker, so stepping down from
    /// 2% lands on 0 rather than wrapping.
    pub fn step(&self, delta: i32) {
        self.change(set_backlight::Change::DeltaPercent(delta));
    }

    fn change(&self, change: set_backlight::Change) {
        self.act(action::Kind::SetBacklight(SetBacklight {
            change: Some(change),
        }));
    }
}
