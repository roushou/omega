//! Where the machine's power is coming from.

use crate::state::Power;

impl Power {
    /// Whether it is running on mains.
    ///
    /// False on a machine that has not said, which is the same answer a
    /// widget wants: draw the battery, not a claim about the wall.
    pub fn on_ac(&self) -> bool {
        self.read().is_some_and(|power| power.on_ac)
    }
}
