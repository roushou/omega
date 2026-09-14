//! Keyboard device and layout state.

crate::wiring::reading! {
    /// Keyboard device and layout state.
    Input: omega_proto::omega::InputState
}

impl Input {
    /// The keyboard's device name.
    pub fn keyboard(&self) -> Option<String> {
        self.read()
            .map(|input| input.keyboard)
            .filter(|name| !name.is_empty())
    }

    /// XKB layout identifier, such as `us` or `fr`.
    pub fn layout(&self) -> Option<String> {
        self.read()
            .map(|input| input.layout)
            .filter(|layout| !layout.is_empty())
    }

    /// Layout display name, such as `English (US)`.
    pub fn keymap(&self) -> Option<String> {
        self.read()
            .map(|input| input.keymap)
            .filter(|keymap| !keymap.is_empty())
    }
}
