use std::ops::BitOr;

/// A logical key after keyboard-layout translation, separate from inserted text.
/// Character case is ignored by matching; Shift remains an explicit modifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Key {
    Character(char),
    Escape,
    Enter,
    Tab,
    Backspace,
    Delete,
    Insert,
    Home,
    End,
    PageUp,
    PageDown,
    ArrowLeft,
    ArrowRight,
    ArrowUp,
    ArrowDown,
    Space,
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,
}

impl Key {
    /// Canonical identity for adapters. Named keys and characters have distinct prefixes.
    pub fn identity(self) -> String {
        match self {
            Self::Character(' ') | Self::Space => "key:Space".into(),
            Self::Character(c) => format!("char:{}", c.to_lowercase()),
            Self::Escape => "key:Escape".into(),
            Self::Enter => "key:Enter".into(),
            Self::Tab => "key:Tab".into(),
            Self::Backspace => "key:Backspace".into(),
            Self::Delete => "key:Delete".into(),
            Self::Insert => "key:Insert".into(),
            Self::Home => "key:Home".into(),
            Self::End => "key:End".into(),
            Self::PageUp => "key:PageUp".into(),
            Self::PageDown => "key:PageDown".into(),
            Self::ArrowLeft => "key:ArrowLeft".into(),
            Self::ArrowRight => "key:ArrowRight".into(),
            Self::ArrowUp => "key:ArrowUp".into(),
            Self::ArrowDown => "key:ArrowDown".into(),
            Self::F1 => "key:F1".into(),
            Self::F2 => "key:F2".into(),
            Self::F3 => "key:F3".into(),
            Self::F4 => "key:F4".into(),
            Self::F5 => "key:F5".into(),
            Self::F6 => "key:F6".into(),
            Self::F7 => "key:F7".into(),
            Self::F8 => "key:F8".into(),
            Self::F9 => "key:F9".into(),
            Self::F10 => "key:F10".into(),
            Self::F11 => "key:F11".into(),
            Self::F12 => "key:F12".into(),
        }
    }
}

/// Shortcut modifiers. Lock states and keypad location are not modifiers.
/// AltGraph is distinct from Control+Alt; adapters must preserve that distinction.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Modifiers(u8);
impl Modifiers {
    pub const NONE: Self = Self(0);
    pub const CONTROL: Self = Self(1);
    pub const SHIFT: Self = Self(2);
    pub const ALT: Self = Self(4);
    pub const META: Self = Self(8);
    pub const ALT_GRAPH: Self = Self(16);
    /// Stable bit representation for adapters.
    pub const fn bits(self) -> u8 {
        self.0
    }
    /// Reject unknown modifier bits at the adapter boundary.
    pub const fn from_bits(bits: u8) -> Option<Self> {
        if bits & !31 == 0 {
            Some(Self(bits))
        } else {
            None
        }
    }
}

impl BitOr for Modifiers {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

/// Native event phase. Repeated presses retain the Press phase.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Phase {
    Press,
    Release,
}

/// Normalized key input. This is not a text-input or composition event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyEvent {
    pub key: Key,
    pub modifiers: Modifiers,
    pub phase: Phase,
    pub repeat: bool,
}

impl KeyEvent {
    pub const fn pressed(key: Key, modifiers: Modifiers) -> Self {
        Self {
            key,
            modifiers,
            phase: Phase::Press,
            repeat: false,
        }
    }
}
