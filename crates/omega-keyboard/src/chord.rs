use crate::{Key, KeyEvent, Modifiers, Phase};

/// A logical key and exact modifier set. Presses only, without auto-repeat, by default.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Chord {
    key: Key,
    modifiers: Modifiers,
    phase: Phase,
    repeats: bool,
}

impl Chord {
    pub const fn new(key: Key) -> Self {
        Self {
            key,
            modifiers: Modifiers::NONE,
            phase: Phase::Press,
            repeats: false,
        }
    }
    pub fn ctrl(self) -> Self {
        self.with(Modifiers::CONTROL)
    }
    pub fn shift(self) -> Self {
        self.with(Modifiers::SHIFT)
    }
    pub fn alt(self) -> Self {
        self.with(Modifiers::ALT)
    }
    pub fn meta(self) -> Self {
        self.with(Modifiers::META)
    }
    /// Add required modifiers; extra event modifiers never match.
    pub fn with(mut self, modifiers: Modifiers) -> Self {
        self.modifiers = self.modifiers | modifiers;
        self
    }
    /// Match release events, independently of prior presses or focus history.
    pub const fn on_release(mut self) -> Self {
        self.phase = Phase::Release;
        self
    }
    /// Allow native auto-repeat events in addition to the initial event.
    pub const fn repeat(mut self) -> Self {
        self.repeats = true;
        self
    }
    pub const fn key(self) -> Key {
        self.key
    }
    pub const fn modifiers(self) -> Modifiers {
        self.modifiers
    }
    pub const fn phase(self) -> Phase {
        self.phase
    }
    pub const fn allows_repeat(self) -> bool {
        self.repeats
    }
    pub fn matches(self, event: &KeyEvent) -> bool {
        self.key.identity() == event.key.identity()
            && self.modifiers == event.modifiers
            && self.phase == event.phase
            && (!event.repeat || self.repeats)
    }
    pub(crate) fn overlaps(self, other: Self) -> bool {
        self.key.identity() == other.key.identity()
            && self.modifiers == other.modifiers
            && self.phase == other.phase
    }
}
