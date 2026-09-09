//! The keys a keybind can name.
//!
//! `Keybind.key` is a string on the wire, and its own schema comment says the
//! SDK wraps it in a closed enum. This is that enum: the names are the keysyms
//! a compositor expects, so `Key::PageUp` is `Prior` and `Key::VolumeUp` is
//! `XF86AudioRaiseVolume` — spellings nobody should have to remember, and
//! nobody should be able to get subtly wrong.
//!
//! Closed like every other taxonomy here. A keyboard has more keys than this;
//! the set grows in the table below when somebody needs one, which is a build
//! error away rather than a bind that silently never fires.

use std::fmt;

/// Declare the keys: the enum, `ALL`, and the keysym each one carries.
macro_rules! keys {
    ($(
        $(#[$meta:meta])*
        $Variant:ident => $keysym:literal,
    )*) => {
        /// A key a bind can be put on.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub enum Key {
            $($(#[$meta])* $Variant,)*
        }

        impl Key {
            /// Every key, in declaration order.
            pub const ALL: &'static [Key] = &[$(Self::$Variant,)*];

            /// The keysym a compositor knows it by.
            pub const fn keysym(self) -> &'static str {
                match self {
                    $(Self::$Variant => $keysym,)*
                }
            }
        }
    };
}

keys! {
    // Letters.
    A => "a",
    B => "b",
    C => "c",
    D => "d",
    E => "e",
    F => "f",
    G => "g",
    H => "h",
    I => "i",
    J => "j",
    K => "k",
    L => "l",
    M => "m",
    N => "n",
    O => "o",
    P => "p",
    Q => "q",
    R => "r",
    S => "s",
    T => "t",
    U => "u",
    V => "v",
    W => "w",
    X => "x",
    Y => "y",
    Z => "z",
    // Digits, as the row above the letters.
    D0 => "0",
    D1 => "1",
    D2 => "2",
    D3 => "3",
    D4 => "4",
    D5 => "5",
    D6 => "6",
    D7 => "7",
    D8 => "8",
    D9 => "9",
    // Function keys.
    F1 => "F1",
    F2 => "F2",
    F3 => "F3",
    F4 => "F4",
    F5 => "F5",
    F6 => "F6",
    F7 => "F7",
    F8 => "F8",
    F9 => "F9",
    F10 => "F10",
    F11 => "F11",
    F12 => "F12",
    // Named keys.
    Space => "space",
    Return => "Return",
    Tab => "Tab",
    Escape => "Escape",
    Backspace => "BackSpace",
    Delete => "Delete",
    Insert => "Insert",
    Home => "Home",
    End => "End",
    PageUp => "Prior",
    PageDown => "Next",
    Left => "Left",
    Right => "Right",
    Up => "Up",
    Down => "Down",
    Print => "Print",
    Menu => "Menu",
    // Punctuation a bind actually reaches for.
    Minus => "minus",
    Equal => "equal",
    BracketLeft => "bracketleft",
    BracketRight => "bracketright",
    Semicolon => "semicolon",
    Apostrophe => "apostrophe",
    Comma => "comma",
    Period => "period",
    Slash => "slash",
    Backslash => "backslash",
    Grave => "grave",
    // The keys a laptop puts on its top row.
    VolumeUp => "XF86AudioRaiseVolume",
    VolumeDown => "XF86AudioLowerVolume",
    VolumeMute => "XF86AudioMute",
    MicMute => "XF86AudioMicMute",
    PlayPause => "XF86AudioPlay",
    NextTrack => "XF86AudioNext",
    PreviousTrack => "XF86AudioPrev",
    BrightnessUp => "XF86MonBrightnessUp",
    BrightnessDown => "XF86MonBrightnessDown",
}

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.keysym())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_key_carries_one_keysym_and_no_two_share_it() {
        let mut syms: Vec<_> = Key::ALL.iter().map(|key| key.keysym()).collect();
        let total = syms.len();
        syms.sort_unstable();
        syms.dedup();
        assert_eq!(syms.len(), total, "two keys share a keysym");
    }

    #[test]
    fn the_spellings_are_the_compositor_s_rather_than_the_obvious_ones() {
        // The whole reason this is a type: nobody remembers that page-up is
        // `Prior`, and a bind spelled `PageUp` would simply never fire.
        assert_eq!(Key::PageUp.keysym(), "Prior");
        assert_eq!(Key::PageDown.keysym(), "Next");
        assert_eq!(Key::VolumeUp.keysym(), "XF86AudioRaiseVolume");
        assert_eq!(Key::Return.keysym(), "Return");
    }
}
