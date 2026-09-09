//! The icon set: the names a unit can ask for, and the glyphs a shell draws
//! them as.
//!
//! One table, for the same reason [`NodeKind`] is one: the names lived in a
//! rustdoc list, a hand-written `Icons.js`, and a test that scraped backticks
//! out of the first to compare against lines parsed out of the second. Three
//! places, agreeing by inspection.
//!
//! Now the enum is the vocabulary. `Icons.js` is generated from it the way
//! `Props.js` is generated from the node table, so a glyph added here reaches
//! the shell or fails the build — and `Icon::new` takes a [`Glyph`], so a
//! name the shell has no glyph for is a compile error rather than a word in
//! somebody's bar.
//!
//! The codepoints are the Font Awesome block (U+F000..U+F2FF), the oldest and
//! most widely present part of every Nerd Font patch: a glyph from here draws
//! under JetBrainsMono, CaskaydiaCove, Hack and the rest alike.
//!
//! [`NodeKind`]: crate::NodeKind

use std::fmt;

/// Declare the icon set: the enum, `ALL`, the wire name, and the glyph.
macro_rules! glyphs {
    ($(
        $(#[$meta:meta])*
        $Variant:ident => $name:literal : $glyph:literal,
    )*) => {
        /// An icon a unit can ask for.
        ///
        /// Closed here, open on the wire: `icon.name` stays a string, so a
        /// shell meeting a name it does not know draws the name itself — a
        /// legible word rather than a blank space. [`Icon::named`] is how a
        /// unit written for a richer shell reaches one this build cannot
        /// name.
        ///
        /// [`Icon::named`]: https://docs.rs/omega-rs
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub enum Glyph {
            $($(#[$meta])* $Variant,)*
        }

        impl Glyph {
            /// Every glyph, in declaration order.
            pub const ALL: &'static [Glyph] = &[$(Self::$Variant,)*];

            /// The name it takes in `icon.name`.
            pub const fn name(self) -> &'static str {
                match self {
                    $(Self::$Variant => $name,)*
                }
            }

            /// The character a shell draws it as.
            pub const fn glyph(self) -> char {
                match self {
                    $(Self::$Variant => $glyph,)*
                }
            }
        }
    };
}

glyphs! {
    // Power.
    Battery => "battery": '\u{f240}',
    BatteryFull => "battery-full": '\u{f240}',
    BatteryThreeQuarters => "battery-three-quarters": '\u{f241}',
    BatteryHalf => "battery-half": '\u{f242}',
    BatteryQuarter => "battery-quarter": '\u{f243}',
    BatteryEmpty => "battery-empty": '\u{f244}',
    Plug => "plug": '\u{f1e6}',
    Power => "power": '\u{f011}',
    // Network.
    Wifi => "wifi": '\u{f1eb}',
    Globe => "globe": '\u{f0ac}',
    Link => "link": '\u{f0c1}',
    Bluetooth => "bluetooth": '\u{f293}',
    Download => "download": '\u{f019}',
    Upload => "upload": '\u{f093}',
    // Sound.
    Volume => "volume": '\u{f028}',
    VolumeUp => "volume-up": '\u{f028}',
    VolumeDown => "volume-down": '\u{f027}',
    VolumeOff => "volume-off": '\u{f026}',
    Headphones => "headphones": '\u{f025}',
    Microphone => "microphone": '\u{f130}',
    MicrophoneOff => "microphone-off": '\u{f131}',
    Music => "music": '\u{f001}',
    // Machine.
    Cpu => "cpu": '\u{f2db}',
    Thermometer => "thermometer": '\u{f2c7}',
    Keyboard => "keyboard": '\u{f11c}',
    Camera => "camera": '\u{f030}',
    Terminal => "terminal": '\u{f120}',
    Cog => "cog": '\u{f013}',
    // Time.
    Clock => "clock": '\u{f017}',
    Calendar => "calendar": '\u{f073}',
    Sun => "sun": '\u{f185}',
    Moon => "moon": '\u{f186}',
    // Status.
    Bell => "bell": '\u{f0f3}',
    Warning => "warning": '\u{f071}',
    Check => "check": '\u{f00c}',
    Close => "close": '\u{f00d}',
    Refresh => "refresh": '\u{f021}',
    Lock => "lock": '\u{f023}',
    Search => "search": '\u{f002}',
    Star => "star": '\u{f005}',
    Heart => "heart": '\u{f004}',
    // Places and people.
    Home => "home": '\u{f015}',
    User => "user": '\u{f007}',
    Folder => "folder": '\u{f07b}',
    Envelope => "envelope": '\u{f0e0}',
    Trash => "trash": '\u{f1f8}',
}

impl Glyph {
    /// The glyph this name asks for, if it is one this build knows.
    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|glyph| glyph.name() == name)
    }
}

impl fmt::Display for Glyph {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_glyph_is_named_once() {
        let mut names: Vec<_> = Glyph::ALL.iter().map(|glyph| glyph.name()).collect();
        let total = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), total, "two glyphs share a name");
    }

    #[test]
    fn a_glyph_round_trips_through_its_wire_name() {
        for glyph in Glyph::ALL {
            assert_eq!(Glyph::parse(glyph.name()), Some(*glyph));
        }
        assert_eq!(Glyph::parse("nothing-this-build-draws"), None);
    }

    #[test]
    fn every_glyph_is_in_the_block_every_nerd_font_patches() {
        // Outside U+F000..U+F2FF a glyph is present in some patches and not
        // others, which is a widget that draws a box on somebody's machine.
        for glyph in Glyph::ALL {
            let point = glyph.glyph() as u32;
            assert!(
                (0xf000..=0xf2ff).contains(&point),
                "{glyph} is U+{point:04X}, outside the Font Awesome block"
            );
        }
    }
}
