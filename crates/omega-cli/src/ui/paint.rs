//! Styling the parts of a line, and the glyphs a terminal can be trusted with.

use std::fmt::Display;
use std::path::{Path, PathBuf};

use anstyle::{AnsiColor, Effects, Style};

/// How the pieces of a message are dressed.
///
/// One accent per line: the thing the reader has to act on is bright, the
/// context around it recedes. A path is not news — it is where the news
/// happened — so paths are dim and never the brightest thing on their line.
#[derive(Debug)]
pub struct Paint;

impl Paint {
    /// An identifier the reader will type or grep for.
    pub fn name(text: impl Display) -> String {
        Self::wrap(Style::new().effects(Effects::BOLD), text)
    }

    /// A command the reader is being told to run.
    pub fn command(text: impl Display) -> String {
        Self::wrap(
            Style::new()
                .fg_color(Some(AnsiColor::Cyan.into()))
                .effects(Effects::BOLD),
            text,
        )
    }

    /// A path, shortened against `$HOME`. `/home/you/.config/omega` is four
    /// times as wide as `~/.config/omega` and says the same thing.
    pub fn path(path: impl AsRef<Path>) -> String {
        Self::dim(Self::abbreviate(path.as_ref()).display())
    }

    /// Context: true, worth having on the line, not what the line is about.
    pub fn dim(text: impl Display) -> String {
        Self::wrap(Style::new().effects(Effects::DIMMED), text)
    }

    /// Why something failed, on the same line as what failed.
    pub fn problem(text: impl Display) -> String {
        Self::wrap(Style::new().fg_color(Some(AnsiColor::Red.into())), text)
    }

    /// `$HOME/x` as `~/x`, and anything else unchanged.
    pub fn abbreviate(path: &Path) -> PathBuf {
        let Ok(home) = std::env::var("HOME") else {
            return path.to_path_buf();
        };

        match path.strip_prefix(&home) {
            Ok(rest) => Path::new("~").join(rest),
            Err(_) => path.to_path_buf(),
        }
    }

    /// `1 unit`, `3 units`, `2 capabilities`: a count nobody has to read as
    /// `unit(s)`.
    pub fn count(amount: usize, noun: &str) -> String {
        match amount {
            1 => format!("1 {noun}"),
            other => format!("{other} {}", Self::plural(noun)),
        }
    }

    /// `1.0 GB`, `894 MB`, `12 kB`: what a directory was costing, in the
    /// units a disk is sold in.
    pub fn size(bytes: u64) -> String {
        const UNITS: [&str; 5] = ["B", "kB", "MB", "GB", "TB"];
        let mut size = bytes as f64;
        let mut unit = 0;
        while size >= 1000.0 && unit < UNITS.len() - 1 {
            size /= 1000.0;
            unit += 1;
        }
        match unit {
            0 => format!("{bytes} B"),
            _ if size < 10.0 => format!("{size:.1} {}", UNITS[unit]),
            _ => format!("{size:.0} {}", UNITS[unit]),
        }
    }

    /// English, as far as the nouns this program uses go: `capability` is
    /// `capabilities`, and everything else takes an `s`.
    fn plural(noun: &str) -> String {
        let consonant_y = noun.ends_with('y')
            && noun
                .chars()
                .nth_back(1)
                .is_some_and(|before| !"aeiou".contains(before));

        if consonant_y {
            format!("{}ies", &noun[..noun.len() - 1])
        } else {
            format!("{noun}s")
        }
    }

    fn wrap(style: Style, text: impl Display) -> String {
        format!("{style}{text}{style:#}")
    }
}

/// The marks used in lists, in the two alphabets a terminal might have.
///
/// Chosen by the same signal as colour: a stream that is not a terminal is
/// being read by something, and something reading `✓` is being made to work
/// for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Glyphs {
    Unicode,
    Ascii,
}

impl Glyphs {
    /// What this terminal can be trusted with.
    pub fn detect() -> Self {
        let dumb = std::env::var("TERM").is_ok_and(|term| term == "dumb");
        if dumb { Self::Ascii } else { Self::Unicode }
    }

    pub fn ok(self) -> &'static str {
        match self {
            Self::Unicode => "✓",
            Self::Ascii => "ok",
        }
    }

    pub fn failed(self) -> &'static str {
        match self {
            Self::Unicode => "✗",
            Self::Ascii => "FAIL",
        }
    }

    /// The mark on each line of an itemised list.
    pub fn bullet(self) -> &'static str {
        match self {
            Self::Unicode => "·",
            Self::Ascii => "-",
        }
    }
}
