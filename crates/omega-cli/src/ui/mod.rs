//! How the CLI speaks.
//!
//! Every line the `omega` binary writes goes through [`Ui`], for two reasons.
//!
//! Data and decoration are different streams. Progress, hints and errors go
//! to stderr the way cargo's do; a command's actual answer goes to stdout, so
//! `omega run battery level | jq` gets the value and not a progress report.
//!
//! And a program that prints from twenty call sites has twenty formats. The
//! rail width, the colours, the glyphs and the shortening of paths are
//! decided once, here, which is what makes them consistent everywhere.

mod paint;
mod step;
mod table;

pub use paint::{Glyphs, Paint};
pub use step::Step;
pub use table::{Align, Cell, Column, Table};

use std::fmt::Display;
use std::io::Write;
use std::sync::{Arc, Mutex};

use anstyle::{Effects, Style};

/// The width of the left rail.
///
/// Cargo's, exactly: `omega build` runs cargo with inherited streams, so its
/// `Compiling` and our `Built` scroll past in the same column of the same
/// terminal. A rail one column off would read as two programs sharing a
/// window.
const RAIL: usize = 12;

/// The CLI's output surface.
pub struct Ui {
    out: Box<dyn Write + Send>,
    err: Box<dyn Write + Send>,
    glyphs: Glyphs,
}

impl std::fmt::Debug for Ui {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Ui").field("glyphs", &self.glyphs).finish()
    }
}

impl Ui {
    /// The real streams. Colour is decided by `anstream`, which reads
    /// `NO_COLOR`, `CLICOLOR_FORCE` and whether anyone is actually looking —
    /// so styling is written unconditionally here and stripped downstream.
    pub fn stdio() -> Self {
        Self {
            out: Box::new(anstream::AutoStream::auto(std::io::stdout())),
            err: Box::new(anstream::AutoStream::auto(std::io::stderr())),
            glyphs: Glyphs::detect(),
        }
    }

    /// A `Ui` that keeps what it was told, with the styling stripped. The
    /// output is the product, so it is worth asserting on.
    pub fn recording() -> (Self, Transcript) {
        let transcript = Transcript::default();
        let ui = Self {
            out: Box::new(anstream::StripStream::new(
                Box::new(transcript.out.clone()) as Box<dyn Write + Send>
            )),
            err: Box::new(anstream::StripStream::new(
                Box::new(transcript.err.clone()) as Box<dyn Write + Send>
            )),
            glyphs: Glyphs::Unicode,
        };
        (ui, transcript)
    }

    pub fn glyphs(&self) -> Glyphs {
        self.glyphs
    }

    // ---- decoration (stderr) ----

    /// A reported step: the verb in the rail, the subject after it.
    pub fn step(&mut self, step: Step, message: impl Display) {
        let style = step.style();
        let label = step.label();
        self.report(format_args!("{style}{label:>RAIL$}{style:#} {message}"));
    }

    /// A line that continues the step above it, indented to where that
    /// step's subject began.
    ///
    /// For what genuinely continues a sentence — an error's causes. A list of
    /// separate things is not a continuation: give each one a verb, or the
    /// rail is twelve columns of nothing.
    pub fn detail(&mut self, message: impl Display) {
        self.report(format_args!("{:RAIL$} {message}", ""));
    }

    /// The result of one thing among many: a mark in the rail, the thing
    /// after it, and — when it went wrong — why, on the same line.
    pub fn item(&mut self, ok: bool, message: impl Display) {
        let (mark, style) = match ok {
            true => (self.glyphs.ok(), Step::Checked.style()),
            false => (self.glyphs.failed(), Step::Failed.style()),
        };
        self.report(format_args!("{style}{mark:>RAIL$}{style:#} {message}"));
    }

    /// Pad `text` so a column of them lines up: a step's subject and the
    /// note beside it.
    ///
    /// The caller does the padding rather than a list type doing it, because
    /// what is being aligned is the *message*, and the message is the
    /// caller's.
    pub fn column(text: &str, width: usize) -> String {
        let padding = " ".repeat(width.saturating_sub(text.chars().count()));
        format!("{text}{padding}")
    }

    /// The width a column of these needs.
    pub fn width<'a>(texts: impl IntoIterator<Item = &'a str>) -> usize {
        texts
            .into_iter()
            .map(|text| text.chars().count())
            .max()
            .unwrap_or_default()
    }

    /// What to do next, as the command to type.
    pub fn next(&mut self, command: &str) {
        self.step(Step::Next, Paint::command(command));
    }

    pub fn warn(&mut self, message: impl Display) {
        self.step(Step::Warning, message);
    }

    /// A failure and everything under it. An error's causes are the half of
    /// it that says what to do, so printing only the top line throws that
    /// away.
    pub fn error(&mut self, error: &anyhow::Error) {
        self.step(Step::Error, error);
        for cause in error.chain().skip(1) {
            self.detail(Paint::dim(format_args!("{} {cause}", self.glyphs.bullet())));
        }
    }

    pub fn blank(&mut self) {
        self.report(format_args!(""));
    }

    // ---- data (stdout) ----

    /// One line of a command's answer.
    pub fn line(&mut self, text: impl Display) {
        let _ = writeln!(self.out, "{text}");
        let _ = self.out.flush();
    }

    /// A unit's own output, passed through exactly as it was written.
    pub fn passthrough(&mut self, text: &str) {
        let _ = write!(self.out, "{text}");
        let _ = self.out.flush();
    }

    /// A table of answers, its headings dimmed so the data is what stands out.
    pub fn table(&mut self, table: &Table) {
        for line in table.render(Style::new().effects(Effects::DIMMED)) {
            self.line(line);
        }
    }

    /// Writing to a closed pipe is not this program's problem: `omega status
    /// | head -1` is a thing people do, and it must not produce an error.
    fn report(&mut self, message: std::fmt::Arguments<'_>) {
        let _ = writeln!(self.err, "{message}");
        let _ = self.err.flush();
    }
}

/// What a recording [`Ui`] was told, per stream.
#[derive(Debug, Default, Clone)]
pub struct Transcript {
    out: Buffer,
    err: Buffer,
}

impl Transcript {
    /// The command's answer.
    pub fn out(&self) -> String {
        self.out.contents()
    }

    /// Everything the reader was told about how it went.
    pub fn err(&self) -> String {
        self.err.contents()
    }
}

/// A buffer two owners can hold: the `Ui` writes it, the test reads it.
#[derive(Debug, Default, Clone)]
struct Buffer(Arc<Mutex<Vec<u8>>>);

impl Buffer {
    fn contents(&self) -> String {
        String::from_utf8_lossy(
            &self
                .0
                .lock()
                .expect("the transcript lock is never poisoned"),
        )
        .into_owned()
    }
}

impl Write for Buffer {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0
            .lock()
            .expect("the transcript lock is never poisoned")
            .extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
