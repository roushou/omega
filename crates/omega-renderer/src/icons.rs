//! Generate the renderer's icon lookup from [`Glyph`].

use std::fmt::Write as _;

use omega_proto::Glyph;

/// The generated icon set.
#[derive(Debug)]
pub struct Icons;

impl Icons {
    /// Where the generated file belongs in the shell tree.
    pub const FILE: &'static str = "Icons.js";

    /// Generate the complete JavaScript file.
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

// GENERATED icon lookup from `omega-proto` by `omega_renderer::Icons`.
// Edit the source table and run `OMEGA_REGENERATE=1 cargo test -p omega-omarchy --test renderer`.
// Glyphs use the Nerd Font Font Awesome range; unknown names display as text.

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
