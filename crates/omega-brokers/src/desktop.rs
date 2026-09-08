//! Launching applications and taking screenshots.
//!
//! Not a subsystem with a connection: two things a desktop does by running a
//! program. A broker rather than the daemon's own because both need to know
//! desktop conventions — where `.desktop` files live, what a Wayland
//! screenshot tool is called — and the daemon's job is not to know that.
//!
//! Everything decided here is pure. Which program a desktop id names, and
//! what a screenshot request becomes on a command line, are both a function
//! over their input, and both are where the mistakes are.

use std::path::PathBuf;

use async_trait::async_trait;
use tokio::process::Command;

use omega_proto::omega::{LaunchApp, Screenshot, StatePatch, action};
use omega_proto::{ActionKind, SystemTopic};

use crate::broker::{Broker, BrokerError};

/// A `.desktop` file, and the command it names.
#[derive(Debug)]
pub struct DesktopEntry;

impl DesktopEntry {
    /// The `Exec` line of a desktop entry, with the field codes taken out.
    ///
    /// The spec lets an `Exec` carry placeholders — `%f` for a file, `%U` for
    /// urls, `%i` for an icon — that a launcher is meant to substitute or
    /// drop. Passing them through launches an editor with a file literally
    /// named `%f`.
    pub fn command(entry: &str) -> Option<String> {
        let line = entry
            .lines()
            .map(str::trim)
            // Only the `[Desktop Entry]` group. An action group further down
            // has its own `Exec`, and taking the first one found could launch
            // "open a new private window" instead of the browser.
            .take_while(|line| !line.starts_with('[') || *line == "[Desktop Entry]")
            .find_map(|line| line.strip_prefix("Exec="))?;

        let cleaned: Vec<&str> = line
            .split_whitespace()
            .filter(|word| !Self::is_field_code(word))
            .collect();

        if cleaned.is_empty() {
            None
        } else {
            Some(cleaned.join(" "))
        }
    }

    /// A placeholder the spec says a launcher substitutes or removes.
    fn is_field_code(word: &str) -> bool {
        matches!(
            word,
            "%f" | "%F"
                | "%u"
                | "%U"
                | "%d"
                | "%D"
                | "%n"
                | "%N"
                | "%i"
                | "%c"
                | "%k"
                | "%v"
                | "%m"
        )
    }

    /// Where a desktop id could be, most specific first: a user's own copy of
    /// an entry wins over the one a package installed.
    pub fn paths(desktop_id: &str) -> Vec<PathBuf> {
        let name = if desktop_id.ends_with(".desktop") {
            desktop_id.to_string()
        } else {
            format!("{desktop_id}.desktop")
        };

        let mut roots = Vec::new();
        if let Ok(home) = std::env::var("XDG_DATA_HOME") {
            roots.push(PathBuf::from(home));
        } else if let Ok(home) = std::env::var("HOME") {
            roots.push(PathBuf::from(home).join(".local/share"));
        }
        let dirs = std::env::var("XDG_DATA_DIRS")
            .unwrap_or_else(|_| "/usr/local/share:/usr/share".to_string());
        roots.extend(
            dirs.split(':')
                .filter(|dir| !dir.is_empty())
                .map(PathBuf::from),
        );

        roots
            .into_iter()
            .map(|root| root.join("applications").join(&name))
            .collect()
    }
}

/// A screenshot request, as a command line.
#[derive(Debug)]
pub struct Capture;

impl Capture {
    /// `grim` is the Wayland screenshot tool; `slurp` selects a region and
    /// `wl-copy` puts the result on the clipboard. Named here rather than in
    /// the daemon because knowing them is a desktop convention.
    pub fn command(shot: &Screenshot) -> Option<String> {
        let mut grim = String::from("grim");

        if !shot.region_monitor_id.is_empty() {
            grim.push_str(&format!(" -o {}", Self::named(&shot.region_monitor_id)?));
        } else if !shot.fullscreen {
            // Neither a monitor nor the whole screen: ask where.
            grim.push_str(" -g \"$(slurp)\"");
        }

        Some(match (shot.clipboard, shot.output_path.is_empty()) {
            // grim writes to stdout when told to, which is what the clipboard
            // wants — and a path as well means both, which grim cannot do in
            // one pass.
            (true, true) => format!("{grim} - | wl-copy"),
            (true, false) => format!(
                "{grim} {path} && wl-copy < {path}",
                path = Self::named(&shot.output_path)?
            ),
            (false, false) => format!("{grim} {}", Self::named(&shot.output_path)?),
            // Nowhere to put it. grim's own default is a dated file in the
            // working directory, which for a daemon is not a place anyone
            // will find it.
            (false, true) => return None,
        })
    }

    /// A name that could be pasted into a shell line.
    ///
    /// These go through a shell — `grim … | wl-copy` is a pipeline — so a
    /// path carrying a quote or a semicolon would be a second command.
    fn named(name: &str) -> Option<&str> {
        if name.is_empty() || name.contains(['\'', '"', ';', '&', '|', '$', '`', '\n']) {
            None
        } else {
            Some(name)
        }
    }
}

#[derive(Debug, Default)]
pub struct Desktop;

impl Desktop {
    pub fn new() -> Self {
        Self
    }

    /// The command a desktop id names, from the first file that has one.
    fn launch(app: &LaunchApp) -> Result<String, BrokerError> {
        for path in DesktopEntry::paths(&app.desktop_id) {
            let Ok(entry) = std::fs::read_to_string(&path) else {
                continue;
            };
            if let Some(command) = DesktopEntry::command(&entry) {
                let arguments = app.args.join(" ");
                return Ok(if arguments.is_empty() {
                    command
                } else {
                    format!("{command} {arguments}")
                });
            }
        }

        Err(BrokerError::unreadable(format!(
            "no desktop entry named {:?}",
            app.desktop_id
        )))
    }

    /// Run a command line, and wait long enough to know it started.
    async fn run(command: &str) -> Result<(), BrokerError> {
        let status = Command::new("sh")
            .arg("-c")
            .arg(command)
            .status()
            .await
            .map_err(|error| BrokerError::Unreadable(error.to_string()))?;

        if status.success() {
            Ok(())
        } else {
            // Loud: a screenshot nobody took is not a screenshot.
            Err(BrokerError::unreadable(format!(
                "{command:?} exited {status}"
            )))
        }
    }
}

#[async_trait]
impl Broker for Desktop {
    fn name(&self) -> &'static str {
        "desktop"
    }

    fn topics(&self) -> &'static [SystemTopic] {
        &[]
    }

    fn actions(&self) -> &'static [ActionKind] {
        &[ActionKind::LaunchApp, ActionKind::Screenshot]
    }

    async fn act(&mut self, action: &action::Kind) -> Result<Option<StatePatch>, BrokerError> {
        let command = match action {
            action::Kind::LaunchApp(app) => Self::launch(app)?,
            action::Kind::Screenshot(shot) => Capture::command(shot).ok_or(
                BrokerError::Unreadable("a screenshot needs somewhere to go".into()),
            )?,
            other => return Err(BrokerError::Unserved(ActionKind::of(other))),
        };

        Self::run(&command).await?;
        Ok(None)
    }
}
