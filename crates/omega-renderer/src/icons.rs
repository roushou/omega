//! Generating the shell's icon set.
//!
//! The same bargain [`Props`] makes, for the same reason: the names a unit
//! can ask for lived in three places — a rustdoc list, a hand-written
//! `Icons.js`, and a test that scraped one to compare against the other.
//! [`Glyph`] is the vocabulary now, and this emits the file from it.
//!
//! [`Props`]: crate::Props

use std::fmt::Write as _;

use omega_proto::Glyph;

/// The generated icon set.
#[derive(Debug)]
pub struct Icons;

impl Icons {
    /// Where the generated file belongs in the shell tree.
    pub const FILE: &'static str = "Icons.js";

    /// The whole file.
    pub fn generate() -> String {
        let mut out = String::new();
        out.push_str(Self::PREAMBLE);

        for glyph in Glyph::ALL {
            let _ = write!(
                out,
                "\n    {:?}: \"\\u{:04x}\",",
                glyph.name(),
                glyph.glyph() as u32
            );
        }

        out.push_str(Self::EPILOGUE);
        out
    }

    const PREAMBLE: &'static str = r#".pragma library

// Icon names, and the glyphs a bar draws them as. GENERATED from
// `omega-proto`'s icon table by `omega_renderer::Icons` — do not edit; add
// the glyph there and regenerate with
// `OMEGA_REGENERATE=1 cargo test -p omega-renderer`.
//
// Omarchy's shell draws icons as characters, not images: the bar's font is
// the fontconfig alias `omarchy font set` writes, which resolves to a Nerd
// Font. So an icon is a lookup from a name a unit can write to a codepoint
// that font carries.
//
// These are the Font Awesome block (U+F000..U+F2FF), which is the oldest and
// most widely present part of every Nerd Font patch — a glyph from here draws
// under JetBrainsMono, CaskaydiaCove, Hack and the rest alike. Written as
// escapes so this file stays ASCII and survives any encoding it is copied
// through.
//
// A name that is not here is drawn as itself by `ViewNode.qml`, so a unit
// asking for an icon this shell has never heard of shows a legible word
// rather than a blank space or a replacement box.

var GLYPHS = {"#;

    const EPILOGUE: &'static str = r#"
}

// The glyph for a name, or "" for one this shell does not draw.
function glyph(name) {
    return Object.prototype.hasOwnProperty.call(GLYPHS, name) ? GLYPHS[name] : ""
}

// Every name this shell draws, sorted.
function names() {
    return Object.keys(GLYPHS).sort()
}
"#;
}
