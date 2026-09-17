//! The verbs the CLI reports in.

use anstyle::{AnsiColor, Effects, Style};

/// Closed set of CLI progress labels, aligned with Cargo output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Healthy,
    Ready,
    Unplaced,
    Waiting,
    Starting,
    Running,
    Restarting,
    Stopped,
    Unknown,

    /// Accepted intent, before a host has necessarily made it visible.
    Requested,
    /// A file or directory the CLI wrote.
    Created,
    /// Existing configuration retained without replacement.
    Kept,
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
    /// A retained build was selected for activation.
    Restored,
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
            Self::Healthy => "Healthy",
            Self::Ready => "Ready",
            Self::Unplaced => "Unplaced",
            Self::Waiting => "Waiting",
            Self::Starting => "Starting",
            Self::Running => "Running",
            Self::Restarting => "Restarting",
            Self::Stopped => "Stopped",
            Self::Unknown => "Unknown",

            Self::Requested => "Requested",
            Self::Created => "Created",
            Self::Kept => "Kept",
            Self::Building => "Building",
            Self::Declared => "Declared",
            Self::Evaluated => "Evaluated",
            Self::Built => "Built",
            Self::Checking => "Checking",
            Self::Checked => "Checked",
            Self::Watching => "Watching",
            Self::Changed => "Changed",
            Self::Restarted => "Restarted",
            Self::Restored => "Restored",
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

    /// Assign status colors; Next leaves the accent to the suggested command.
    pub fn style(self) -> Style {
        let color = match self {
            Self::Healthy => Some(AnsiColor::Green),
            Self::Ready => Some(AnsiColor::Green),
            Self::Unplaced => Some(AnsiColor::Cyan),
            Self::Waiting => Some(AnsiColor::Yellow),
            Self::Starting => Some(AnsiColor::Cyan),
            Self::Running => Some(AnsiColor::Green),
            Self::Restarting => Some(AnsiColor::Yellow),
            Self::Stopped => Some(AnsiColor::Yellow),
            Self::Unknown => Some(AnsiColor::Yellow),

            Self::Created
            | Self::Kept
            | Self::Built
            | Self::Checked
            | Self::Restarted
            | Self::Restored
            | Self::Linked
            | Self::Installed
            | Self::Removed
            | Self::Enabled
            | Self::Adopted
            | Self::Released
            | Self::Done => Some(AnsiColor::Green),
            Self::Requested
            | Self::Building
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
