//! Read captured plugin logs.

use std::io::SeekFrom;
use std::path::Path;
use std::time::Duration;

use anyhow::{Context, bail};
use tokio::io::{AsyncBufReadExt, AsyncSeekExt, BufReader};

use omega_host::Layout;
use omega_host::workspace::Plugins;
use omega_proto::UnitName;

use crate::ui::{Paint, Step, Ui};

/// Print captured plugin stdout and stderr. Works even when the daemon is stopped.
#[derive(clap::Args, Debug)]
pub struct LogsCmd {
    /// The unit to read. Omit to list the units that have logs.
    pub unit: Option<String>,

    /// How many lines of history to print.
    #[arg(long, short = 'n', default_value_t = 50)]
    pub lines: usize,

    /// Keep printing as the unit writes.
    #[arg(long, short)]
    pub follow: bool,
}

impl LogsCmd {
    /// How often a follow checks for more output.
    const POLL: Duration = Duration::from_millis(200);

    pub async fn run(self, ui: &mut Ui) -> anyhow::Result<()> {
        let layout = Layout::resolve();

        let Some(unit) = self.unit.as_deref() else {
            return Self::list(&layout, ui);
        };

        let name = UnitName::try_from(unit)?;
        let path = layout.unit_log(&name);
        if !path.exists() {
            bail!("no log for {name} at {} — has it run?", Paint::path(&path));
        }

        // Preserve plugin output bytes without CLI decoration.
        let tail = Self::tail(&path, self.lines)?;
        ui.passthrough(&tail);

        if self.follow {
            Self::follow(&path, ui).await
        } else {
            Ok(())
        }
    }

    /// Read the last lines from the size-bounded unit log.
    fn tail(path: &Path, lines: usize) -> anyhow::Result<String> {
        let contents = std::fs::read_to_string(path)
            .with_context(|| format!("cannot read {}", path.display()))?;

        let start = contents.lines().count().saturating_sub(lines);

        Ok(contents
            .lines()
            .skip(start)
            .map(|line| format!("{line}\n"))
            .collect())
    }

    /// Follow appended output and reset the read offset if the file is truncated.
    async fn follow(path: &Path, ui: &mut Ui) -> anyhow::Result<()> {
        let file = tokio::fs::File::open(path).await?;
        let mut position = file.metadata().await?.len();
        let mut reader = BufReader::new(file);
        reader.seek(SeekFrom::Start(position)).await?;

        loop {
            let mut line = String::new();
            match reader.read_line(&mut line).await? {
                0 => {
                    // Restart the read offset after log truncation.
                    let length = tokio::fs::metadata(path).await?.len();
                    if length < position {
                        let file = tokio::fs::File::open(path).await?;
                        reader = BufReader::new(file);
                        position = 0;
                        continue;
                    }
                    tokio::time::sleep(Self::POLL).await;
                }
                read => {
                    position += read as u64;
                    ui.passthrough(&line);
                }
            }
        }
    }

    /// List plugins with existing log files.
    fn list(layout: &Layout, ui: &mut Ui) -> anyhow::Result<()> {
        let logged: Vec<String> = Plugins::discover(layout)
            .map(|units| {
                units
                    .iter()
                    .filter(|name| layout.unit_log(name).exists())
                    .map(|name| name.to_string())
                    .collect()
            })
            .unwrap_or_default();

        if logged.is_empty() {
            ui.warn("no unit has written a log yet");
            ui.next("omega daemon");
        } else {
            // The names are the answer, one per line, so the list can be
            // piped; where they live is a footnote about the answer.
            for unit in &logged {
                ui.line(unit);
            }
            ui.step(
                Step::Done,
                format!(
                    "{} in {}",
                    Paint::count(logged.len(), "log"),
                    Paint::path(layout.logs_dir())
                ),
            );
        }
        Ok(())
    }
}
