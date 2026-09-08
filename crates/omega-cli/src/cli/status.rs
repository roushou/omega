//! `omega status`: what the daemon is running, and what keeps falling over.

use std::time::Duration;

use anstyle::{AnsiColor, Effects, Style};
use anyhow::{Context, bail};
use tokio::io::{AsyncBufReadExt, BufReader};

use omega_proto::omega::{UnitPhase, UnitStatus, state_topic};
use omega_proto::{Observation, Socket, SystemTopic};

use crate::ui::{Cell, Column, Paint, Table, Ui};

/// Report the daemon's view of every unit.
#[derive(Debug, clap::Args)]
pub struct StatusCmd;

impl StatusCmd {
    /// How long to wait for the daemon's opening snapshot.
    const TIMEOUT: Duration = Duration::from_secs(2);

    pub async fn run(self, ui: &mut Ui) -> anyhow::Result<()> {
        let socket = Observation::socket();
        if !socket.is_live() {
            bail!(
                "no daemon is listening on {} — start one with {}",
                Paint::path(socket.path()),
                Paint::command("omega daemon")
            );
        }

        let units = tokio::time::timeout(Self::TIMEOUT, Self::read_units(&socket))
            .await
            .context("the daemon did not report its units in time")??;

        if units.is_empty() {
            ui.warn("the daemon is running no units");
            ui.next("omega build");
        } else {
            ui.table(&Self::table(&units));
        }
        Ok(())
    }

    /// Read the observation socket until the `units` topic arrives. The
    /// daemon sends its whole current state on connect, so this is one
    /// round-trip, not a subscription.
    async fn read_units(socket: &Socket) -> anyhow::Result<Vec<UnitStatus>> {
        let stream = socket.connect_stream().await?;
        let mut lines = BufReader::new(stream).lines();

        // Views and topics share the stream, so most lines are not this one.
        while let Some(line) = lines.next_line().await? {
            let Some(topic) = Observation::topic(&line) else {
                continue;
            };
            if topic.topic != *SystemTopic::Units.as_str() {
                continue;
            }
            match topic.value {
                Some(state_topic::Value::Units(units)) => return Ok(units.units),
                other => bail!("the daemon sent a units topic carrying {other:?}"),
            }
        }

        Ok(Vec::new())
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
