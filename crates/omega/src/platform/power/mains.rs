//! External power connection state.

crate::wiring::reading! {
    /// External power connection state.
    Mains: omega_proto::omega::MainsState
}

impl Mains {
    /// Whether external power is connected. Returns `false` without a reading.
    pub fn is_connected(&self) -> bool {
        self.read().is_some_and(|mains| mains.connected)
    }
}
