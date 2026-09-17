//! Read captured plugin logs.

use std::io::SeekFrom;
use std::path::Path;
use std::time::Duration;

use anyhow::{Context, bail};
use tokio::io::{AsyncBufReadExt, AsyncSeekExt, BufReader};

use omega_host::Layout;
use omega_host::workspace::Plugins;
use omega_proto::PluginName;

use crate::ui::{Paint, Step, Ui};

/// Print captured plugin stdout and stderr. Works even when the daemon is stopped.
#[derive(clap::Args, Debug)]
pub struct LogsCmd {
    /// The plugin to read. Omit to list the plugins that have logs.
    #[arg(value_name = "PLUGIN")]
    pub plugin_name: Option<PluginName>,

    /// How many lines of history to print.
    #[arg(long, short = 'n', default_value_t = 50)]
    pub lines: usize,

    /// Keep printing as the plugin writes.
    #[arg(long, short)]
    pub follow: bool,
}

impl LogsCmd {
    /// How often a follow checks for more output.
    const POLL: Duration = Duration::from_millis(200);

    pub async fn run(self, ui: &mut Ui) -> anyhow::Result<()> {
        let layout = Layout::resolve();

        let Some(plugin_name) = self.plugin_name.as_ref() else {
            return Self::list(&layout, ui);
        };

        let path = layout.plugin_log(plugin_name);
        if !path.exists() {
            bail!(
                "no log for {plugin_name} at {} — has it run?",
                Paint::path(&path)
            );
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

    /// Read the last lines from the size-bounded plugin log.
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
        let plugins = Plugins::discover(layout)
            .with_context(|| format!("cannot discover plugins in {}", layout.config.display()))?;
        let logged: Vec<String> = plugins
            .iter()
            .filter(|name| layout.plugin_log(name).exists())
            .map(|name| name.to_string())
            .collect();

        if logged.is_empty() {
            ui.warn("no plugin has written a log yet");
            ui.next("omega daemon");
        } else {
            // The names are the answer, one per line, so the list can be
            // piped; where they live is a footnote about the answer.
            for plugin in &logged {
                ui.line(plugin);
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

#[cfg(test)]
mod tests {
    use super::*;
    use omega_host::{AtomicFile, TempPath};

    struct Fixture(Layout);

    impl Fixture {
        fn new() -> Self {
            let root = TempPath::sibling(Path::new("/tmp/omega-logs"), "test");
            Self(Layout::at(&root, root.join("state"), root.join("cache")))
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0.config);
        }
    }

    #[test]
    fn discovery_failures_are_not_reported_as_empty_logs() {
        let fixture = Fixture::new();
        for manifest in [
            None,
            Some("[workspace"),
            Some("[package]\nname = 'example'\n"),
        ] {
            if let Some(source) = manifest {
                AtomicFile::at(fixture.0.workspace_manifest())
                    .write(source.as_bytes())
                    .unwrap();
            }
            let (mut ui, transcript) = Ui::recording();
            let error = LogsCmd::list(&fixture.0, &mut ui).unwrap_err();
            assert!(error.to_string().contains("cannot discover plugins"));
            assert!(
                error
                    .downcast_ref::<omega_host::workspace::PluginsError>()
                    .is_some()
            );
            assert!(transcript.out().is_empty());
            assert!(transcript.err().is_empty());
        }
    }

    #[test]
    fn valid_empty_workspace_reports_no_logs() {
        let fixture = Fixture::new();
        AtomicFile::at(fixture.0.workspace_manifest())
            .write(b"[workspace]\nmembers = []\n")
            .unwrap();
        let (mut ui, transcript) = Ui::recording();
        LogsCmd::list(&fixture.0, &mut ui).unwrap();
        assert!(transcript.out().is_empty());
        assert!(transcript.err().contains("no plugin has written a log yet"));
    }
}
