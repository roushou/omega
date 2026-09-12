//! `omega status`: what the daemon is running, and what keeps falling over.

use std::time::Duration;

use anstyle::{AnsiColor, Effects, Style};
use anyhow::Context;

use omega_proto::omega::{UnitPhase, UnitStatus};

use crate::ui::{Cell, Column, Paint, Step, Table, Ui};

/// Report the daemon's view of every unit.
#[derive(Debug, clap::Args)]
pub struct StatusCmd {
    /// Show CLI, daemon, renderer, and resolved config dependency versions.
    #[arg(long)]
    pub versions: bool,
    /// Print the daemon snapshot as JSON for scripts.
    #[arg(long, conflicts_with = "versions")]
    pub json: bool,
}

impl StatusCmd {
    /// How long to wait for the daemon's opening snapshot.
    const TIMEOUT: Duration = Duration::from_secs(2);

    pub async fn run(self, ui: &mut Ui) -> anyhow::Result<()> {
        if self.versions {
            Self::versions(ui).await;
        }
        let status =
            tokio::time::timeout(Self::TIMEOUT, crate::operator::Operator::new().deployment())
                .await
                .context("the daemon did not report status in time")?
                .context("cannot read daemon status; ensure omega daemon is running")?;
        if self.json {
            ui.line(serde_json::to_string(&status)?);
            return Ok(());
        }
        let layout = omega_host::Layout::resolve();
        let published = match omega_host::Generations::new(&layout).pin_current() {
            Ok(generation) => generation,
            Err(error) => {
                ui.warn(format!("published build unavailable: {error}"));
                None
            }
        };
        ui.deployment(
            &status,
            published
                .as_ref()
                .map(|generation| generation.id().as_str()),
        );
        let units = status.units;

        if units.is_empty() {
            ui.step(Step::Checked, "the daemon is running; no plugins");
        } else {
            ui.table(&Self::table(&units));
        }
        Ok(())
    }

    async fn versions(ui: &mut Ui) {
        let version = env!("CARGO_PKG_VERSION");
        ui.step(Step::Checking, format!("CLI {version}"));
        match std::env::current_exe() {
            Ok(path) => ui.detail(format!("executable: {}", Paint::path(path))),
            Err(error) => ui.warn(format!("CLI executable path unavailable: {error}")),
        }
        match tokio::time::timeout(
            Self::TIMEOUT,
            crate::operator::Operator::new().daemon_version(),
        )
        .await
        {
            Ok(Ok(daemon)) if daemon == version => {
                ui.step(Step::Checked, format!("daemon {daemon}"))
            }
            Ok(Ok(daemon)) => ui.warn(format!("daemon {daemon}; CLI {version}")),
            Ok(Err(error)) => ui.warn(format!("daemon version unavailable: {error}")),
            Err(_) => ui.warn("daemon version request timed out"),
        }
        if let Some(shell) = omega_renderer::HostShell::detect() {
            for renderer in omega_renderer::Renderer::ALL {
                use omega_renderer::Installed;
                match renderer.installed(&shell.plugins()) {
                    Installed::Current => ui.step(
                        Step::Checked,
                        format!(
                            "{} {} matches this CLI",
                            renderer.id,
                            omega_renderer::Renderer::VERSION
                        ),
                    ),
                    Installed::Missing => ui.warn(format!("{} is not installed", renderer.id)),
                    Installed::Linked(path) => ui.step(
                        Step::Linked,
                        format!("{} from {}", renderer.id, Paint::path(path)),
                    ),
                    installed @ Installed::Stale { .. } => ui.warn(format!(
                        "{}: {}",
                        renderer.id,
                        installed
                            .difference()
                            .expect("stale renderer has a difference")
                    )),
                }
            }
        } else {
            ui.detail("No supported shell detected; installed renderer unavailable.");
        }
        let layout = omega_host::Layout::resolve();
        if !layout.workspace_manifest().exists() {
            ui.detail("No configuration workspace.");
            return;
        }
        match tokio::time::timeout(
            Duration::from_secs(10),
            crate::cargo::Cargo::new(&layout).packages(),
        )
        .await
        {
            Ok(Ok(mut packages)) => {
                ui.step(Step::Checking, "resolved configuration dependencies");
                packages.sort_by(|a, b| (&a.name, &a.version).cmp(&(&b.name, &b.version)));
                for package in packages
                    .into_iter()
                    .filter(|p| p.name.starts_with("omega-"))
                {
                    let source = match package.source {
                        Some(source) => source,
                        None => format!("path {}", Paint::path(package.manifest_path)),
                    };
                    ui.step(
                        Step::Checking,
                        format!("{} {} — {source}", package.name, package.version),
                    );
                }
            }
            Ok(Err(error)) => ui.warn(format!("config dependency versions unavailable: {error}")),
            Err(_) => ui.warn("config dependency version lookup timed out"),
        }
    }

    /// One row per unit. Phase carries the colour, because it is the column
    /// a reader scans; a restart count of zero is dimmed so that a count
    /// which is not zero is the thing their eye lands on.
    fn table(units: &[UnitStatus]) -> Table {
        let mut table = Table::new(vec![
            Column::left("UNIT"),
            Column::left("PHASE"),
            Column::right("RESTARTS"),
            Column::left("DETAIL"),
        ]);

        for status in units {
            let phase = Self::phase(status.phase);
            table.row(vec![
                Cell::plain(&status.unit),
                Cell::styled(phase.label(), phase.style()),
                Cell::styled(status.restarts, Self::restarts(status.restarts)),
                Cell::styled(&status.detail, Style::new().effects(Effects::DIMMED)),
            ]);
        }

        table.drop_empty(3);
        table
    }

    fn restarts(count: u32) -> Style {
        match count {
            0 => Style::new().effects(Effects::DIMMED),
            _ => Style::new().fg_color(Some(AnsiColor::Yellow.into())),
        }
    }

    fn phase(phase: i32) -> Phase {
        match UnitPhase::try_from(phase) {
            Ok(UnitPhase::Starting) => Phase::Starting,
            Ok(UnitPhase::Running) => Phase::Running,
            Ok(UnitPhase::Restarting) => Phase::Restarting,
            Ok(UnitPhase::Failed) => Phase::Failed,
            Ok(UnitPhase::Stopped) => Phase::Stopped,
            Ok(UnitPhase::Unspecified) | Err(_) => Phase::Unknown,
        }
    }
}

/// A unit's phase as the reader sees it: a word and the colour that word is
/// worth. Decided here rather than at the cell, so `running` cannot be green
/// in one column and plain in another.
#[derive(Debug, Clone, Copy)]
enum Phase {
    Starting,
    Running,
    Restarting,
    Failed,
    Stopped,
    Unknown,
}

impl Phase {
    fn label(self) -> &'static str {
        match self {
            Self::Starting => "starting",
            Self::Running => "running",
            Self::Restarting => "restarting",
            Self::Failed => "failed",
            Self::Stopped => "stopped",
            Self::Unknown => "unknown",
        }
    }

    fn style(self) -> Style {
        match self {
            Self::Running => Style::new()
                .fg_color(Some(AnsiColor::Green.into()))
                .effects(Effects::BOLD),
            Self::Starting | Self::Restarting => {
                Style::new().fg_color(Some(AnsiColor::Yellow.into()))
            }
            Self::Failed => Style::new()
                .fg_color(Some(AnsiColor::Red.into()))
                .effects(Effects::BOLD),
            Self::Stopped | Self::Unknown => Style::new().effects(Effects::DIMMED),
        }
    }
}
