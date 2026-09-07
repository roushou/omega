//! The verbs the CLI reports in.

use anstyle::{AnsiColor, Effects, Style};

/// One reported step, closed so that adding a verb is a decision made here
/// rather than a string typed at a call site.
///
/// The labels are cargo's grammar on purpose: `omega build` runs cargo with
/// inherited streams, so `Compiling` and `Built` scroll past in the same
/// column of the same terminal. Two vocabularies would read as two programs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// A file or directory the CLI wrote.
    Created,
    /// Work starting, which cargo's own output follows.
    Building,
    /// The plugins were asked what they declare, and answered.
    Declared,
    /// The config plane ran and answered.
    Evaluated,
    /// Work finished.
    Built,
    /// Validation starting.
    Checking,
    /// Validation finished, everything sound.
    Checked,
    /// A watch is in place; the command will not return.
    Watching,
    /// The watch fired.
    Changed,
    /// A unit was cycled.
    Restarted,
    /// A config was pointed at where its crates come from.
    Linked,
    /// A renderer was put where the host shell will find it.
    Installed,
    /// It was taken away again.
    Removed,
    /// A widget was put where it will be seen.
    Enabled,
    /// This process has taken a unit's place.
    Adopted,
    /// It gave the unit back.
    Released,
    /// The request was carried out and had nothing to say.
    Done,
    /// What to do next, which is a hint and not a result.
    Next,
    /// Something worth knowing that is not a failure.
    Warning,
    /// A step that did not do what it was asked.
    Failed,
    /// The command is over and it did not work.
    Error,
}

impl Step {
    pub fn label(self) -> &'static str {
        match self {
            Self::Created => "Created",
            Self::Building => "Building",
            Self::Declared => "Declared",
            Self::Evaluated => "Evaluated",
            Self::Built => "Built",
            Self::Checking => "Checking",
            Self::Checked => "Checked",
            Self::Watching => "Watching",
            Self::Changed => "Changed",
            Self::Restarted => "Restarted",
            Self::Linked => "Linked",
            Self::Installed => "Installed",
            Self::Removed => "Removed",
            Self::Enabled => "Enabled",
            Self::Adopted => "Adopted",
            Self::Released => "Released",
            Self::Done => "Done",
            Self::Next => "Next",
            Self::Warning => "Warning",
            Self::Failed => "Failed",
            Self::Error => "Error",
        }
    }

    /// Green finished it, cyan is doing it, red and yellow are the two ways
    /// it went wrong. `Next` is deliberately uncoloured: the command it
    /// points at carries the accent, and one accent per line is the rule.
    pub fn style(self) -> Style {
        let color = match self {
            Self::Created
            | Self::Built
            | Self::Checked
            | Self::Restarted
            | Self::Linked
            | Self::Installed
            | Self::Removed
            | Self::Enabled
            | Self::Adopted
            | Self::Released
            | Self::Done => Some(AnsiColor::Green),
            Self::Building
            | Self::Declared
            | Self::Evaluated
            | Self::Checking
            | Self::Watching
            | Self::Changed => Some(AnsiColor::Cyan),
            Self::Warning => Some(AnsiColor::Yellow),
            Self::Failed | Self::Error => Some(AnsiColor::Red),
            Self::Next => None,
        };

        match color {
            Some(color) => Style::new()
                .fg_color(Some(color.into()))
                .effects(Effects::BOLD),
            None => Style::new().effects(Effects::BOLD),
        }
    }
}
