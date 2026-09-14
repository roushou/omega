//! Whether the machine is plugged in.

crate::wiring::reading! {
    /// Whether it is plugged in.
    Mains: omega_proto::omega::MainsState
}

impl Mains {
    /// Whether it is running on mains.
    ///
    /// False on a machine that has not said, which is the answer a widget
    /// wants anyway: draw the battery, not a claim about the wall.
    pub fn is_connected(&self) -> bool {
        self.read().is_some_and(|mains| mains.connected)
    }
}
