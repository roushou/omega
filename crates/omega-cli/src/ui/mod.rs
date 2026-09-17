//! CLI output formatting.
//! Progress and diagnostics go to stderr; command results go to stdout.
//! All output uses this module's shared alignment, styling, and path formatting.

mod deployment;
mod diagnostic;
mod health;
mod paint;
mod shell;
mod step;
mod table;

pub use paint::{Glyphs, Paint};
pub use step::Step;
pub use table::{Align, Cell, Column, Table};

use std::fmt::Display;
use std::io::Write;
use std::sync::{Arc, Mutex};

use anstyle::{Effects, Style};

/// Match Cargo's twelve-column progress rail.
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
    /// Use `anstream` for terminal detection and color-environment handling.
    pub fn stdio() -> Self {
        Self {
            out: Box::new(anstream::AutoStream::auto(std::io::stdout())),
            err: Box::new(anstream::AutoStream::auto(std::io::stderr())),
            glyphs: Glyphs::detect(),
        }
    }

    /// Capture unstyled stdout and stderr for assertions.
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

    /// Continue the preceding diagnostic at the subject indentation.
    /// Use separate steps for independent items.
    pub fn detail(&mut self, message: impl Display) {
        self.report(format_args!("{:RAIL$} {message}", ""));
    }

    /// The result of one thing among many: a mark in the rail, the thing
    /// after it, and — when it went wrong — why, on the same line.
    pub fn item(&mut self, ok: bool, message: impl Display) {
        let (mark, style) = if ok {
            (self.glyphs.ok(), Step::Checked.style())
        } else {
            (self.glyphs.failed(), Step::Failed.style())
        };
        self.report(format_args!("{style}{mark:>RAIL$}{style:#} {message}"));
    }

    /// Pad a message column to the requested width.
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

    /// Render an error and its cause chain.
    pub fn error(&mut self, error: &anyhow::Error) {
        if let Some(rendered) = diagnostic::CliDiagnostic::render(error, self.glyphs) {
            let mut lines = rendered.lines();
            if let Some(first) = lines.next() {
                self.step(Step::Error, first);
            }
            for line in lines {
                self.detail(line);
            }
            return;
        }
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

    /// A plugin's own output, passed through exactly as it was written.
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

    /// Treat a closed output pipe as successful termination.
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

    /// Captured progress and diagnostics.
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

impl omega_base::execution::Observer for Ui {
    fn observe(&mut self, report: &omega_base::execution::Report) {
        use omega_base::execution::Outcome;
        let description = &report.description.title;
        match &report.outcome {
            Outcome::Running => self.step(Step::Checking, description),
            Outcome::Completed => self.step(Step::Done, description),
            Outcome::Skipped(reason) => self.detail(format!("{description}: {reason}")),
            Outcome::Failed(_) => self.step(Step::Failed, description),
            Outcome::Interrupted => self.warn(format!(
                "{description} interrupted; effects may have completed"
            )),
        }
    }

    fn detail(
        &mut self,
        _: &omega_base::execution::Description,
        detail: &omega_base::execution::Detail,
    ) {
        match &detail.path {
            Some(path) => self.detail(format!("{} {}", detail.message, Paint::path(path))),
            None => self.detail(&detail.message),
        }
    }
}

pub(crate) struct PipelineResult;

impl PipelineResult {
    pub(crate) fn finish<T>(
        result: Result<T, omega_base::execution::Failure<anyhow::Error>>,
    ) -> anyhow::Result<T> {
        result.map_err(|failure| {
            let error = match failure.cause {
                omega_base::execution::FailureCause::Operation(error) => error,
                omega_base::execution::FailureCause::Unconfigured => {
                    anyhow::anyhow!("no implementation configured for {}", failure.step.id.0)
                }
            };
            error.context(format!("pipeline stopped at: {}", failure.step.title))
        })
    }
}
