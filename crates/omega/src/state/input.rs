//! How the machine is being typed at.

use crate::state::Input;

impl Input {
    /// The keyboard's device name.
    pub fn keyboard(&self) -> Option<String> {
        self.read()
            .map(|input| input.keyboard)
            .filter(|name| !name.is_empty())
    }

    /// The layout as xkb spells it: `us`, `fr`. What a bar slot shows.
    pub fn layout(&self) -> Option<String> {
        self.read()
            .map(|input| input.layout)
            .filter(|layout| !layout.is_empty())
    }

    /// The layout as a person reads it: `English (US)`.
    pub fn keymap(&self) -> Option<String> {
        self.read()
            .map(|input| input.keymap)
            .filter(|keymap| !keymap.is_empty())
    }
}
