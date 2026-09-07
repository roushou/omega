//! What the CLI's output promises, asserted rather than eyeballed.
//!
//! The output is the product: it is what a person sees of omega before they
//! see anything else, and a regression in it is silent everywhere but on
//! their screen.

use std::path::Path;

use anstyle::{AnsiColor, Effects, Style};
use omega_cli::ui::{Cell, Column, Paint, Step, Table, Ui};

/// The rail cargo uses. `omega build` runs cargo with inherited streams, so a
/// rail one column off would read as two programs sharing a terminal.
const CARGO_RAIL: usize = 12;

#[test]
fn every_verb_lands_in_the_same_column_cargo_uses() {
    let (mut ui, transcript) = Ui::recording();

    for step in [Step::Created, Step::Building, Step::Restarted, Step::Error] {
        ui.step(step, "subject");
    }

    for line in transcript.err().lines() {
        let (rail, subject) = line.split_at(CARGO_RAIL);
        assert_eq!(rail.trim_start().len(), rail.trim().len(), "{line:?}");
        assert_eq!(subject, " subject", "{line:?}");
    }
}

#[test]
fn a_detail_line_starts_where_its_step_subject_started() {
    let (mut ui, transcript) = Ui::recording();

    ui.step(Step::Created, "a workspace");
    ui.detail("a file in it");

    let written = transcript.err();
    let lines: Vec<&str> = written.lines().collect();
    let subject = lines[0].find("a workspace").unwrap();
    let detail = lines[1].find("a file in it").unwrap();
    assert_eq!(subject, detail, "{lines:?}");
}

#[test]
fn a_column_of_messages_lines_up() {
    let paths = ["units/battery/src/lib.rs", "system/src/main.rs"];
    let width = Ui::width(paths);

    let padded: Vec<String> = paths
        .iter()
        .map(|path| format!("{}  note", Ui::column(path, width)))
        .collect();

    // Aligning happens inside the message, because the message is what has
    // columns in it. The rail is for verbs.
    let note = padded[0].find("note").unwrap();
    assert_eq!(padded[1].find("note"), Some(note), "{padded:?}");
}

#[test]
fn answers_go_to_stdout_and_everything_else_to_stderr() {
    let (mut ui, transcript) = Ui::recording();

    ui.step(Step::Done, "asked");
    ui.line("42");
    ui.next("omega build");

    // `omega run battery level | jq` has to receive a value, not a report.
    assert_eq!(transcript.out(), "42\n");
    assert!(transcript.err().contains("asked"));
    assert!(transcript.err().contains("omega build"));
    assert!(!transcript.out().contains("omega build"));
}

#[test]
fn a_recorded_transcript_carries_no_escape_sequences() {
    let (mut ui, transcript) = Ui::recording();

    ui.step(Step::Created, Paint::name("battery"));
    ui.item(false, Paint::problem("it broke"));

    let written = format!("{}{}", transcript.out(), transcript.err());
    assert!(!written.contains('\u{1b}'), "{written:?}");
    assert!(written.contains("battery"));
    assert!(written.contains("it broke"));
}

#[test]
fn a_path_under_home_is_shortened_to_a_tilde() {
    let home = std::env::var("HOME").expect("a test runs with a home directory");
    let shown = Paint::abbreviate(&Path::new(&home).join(".config/omega"));

    assert_eq!(shown, Path::new("~/.config/omega"));
    // Anything outside it is left exactly as it is.
    assert_eq!(
        Paint::abbreviate(Path::new("/etc/omega")),
        Path::new("/etc/omega")
    );
}

#[test]
fn a_count_reads_as_english() {
    assert_eq!(Paint::count(0, "unit"), "0 units");
    assert_eq!(Paint::count(1, "unit"), "1 unit");
    assert_eq!(Paint::count(3, "unit"), "3 units");

    // As far as the nouns this program counts: a build declares
    // capabilities, not capabilitys.
    assert_eq!(Paint::count(2, "capability"), "2 capabilities");
    assert_eq!(Paint::count(1, "capability"), "1 capability");
    assert_eq!(Paint::count(2, "key"), "2 keys");
}

#[test]
fn an_errors_causes_are_printed_under_it() {
    let (mut ui, transcript) = Ui::recording();

    let error = anyhow::anyhow!("the disk is full")
        .context("cannot write unit.toml")
        .context("the build failed");
    ui.error(&error);

    let written = transcript.err();
    // The cause is the half of an error that says what to do about it.
    assert!(written.contains("the build failed"), "{written}");
    assert!(written.contains("cannot write unit.toml"), "{written}");
    assert!(written.contains("the disk is full"), "{written}");
}

#[test]
fn a_styled_cell_does_not_skew_its_column() {
    let mut table = Table::new(vec![
        Column::left("UNIT"),
        Column::left("PHASE"),
        Column::right("RESTARTS"),
    ]);
    table.row(vec![
        Cell::plain("battery"),
        Cell::styled(
            "running",
            Style::new()
                .fg_color(Some(AnsiColor::Green.into()))
                .effects(Effects::BOLD),
        ),
        Cell::plain(0),
    ]);
    table.row(vec![
        Cell::plain("a-much-longer-unit"),
        Cell::plain("failed"),
        Cell::plain(12),
    ]);

    let (mut ui, transcript) = Ui::recording();
    ui.table(&table);

    // Escapes are characters that no terminal draws; padding measured on them
    // is padding that does not line up.
    let written = transcript.out();
    let lines: Vec<&str> = written.lines().collect();
    let phase = lines[0].find("PHASE").unwrap();
    assert_eq!(lines[1].find("running"), Some(phase), "{lines:?}");
    assert_eq!(lines[2].find("failed"), Some(phase), "{lines:?}");

    // A right-aligned column ends together, whatever its cells are worth.
    assert!(lines[1].ends_with('0') && lines[2].ends_with("12"));
    assert_eq!(lines[1].len(), lines[2].len(), "{lines:?}");
}

#[test]
fn a_column_nothing_filled_in_is_not_shown() {
    let mut table = Table::new(vec![Column::left("UNIT"), Column::left("DETAIL")]);
    table.row(vec![Cell::plain("battery"), Cell::plain("")]);
    table.row(vec![Cell::plain("clock"), Cell::plain("")]);
    table.drop_empty(1);

    let (mut ui, transcript) = Ui::recording();
    ui.table(&table);

    // An empty heading over an empty column is a question the reader has to
    // answer for themselves.
    assert!(!transcript.out().contains("DETAIL"), "{}", transcript.out());
    assert!(transcript.out().contains("battery"));
}

#[test]
fn a_table_row_carries_no_trailing_whitespace() {
    let mut table = Table::new(vec![Column::left("UNIT"), Column::left("DETAIL")]);
    // A styled empty cell is the trap: its escapes are not whitespace, so a
    // row ending in one ends in a gap that trimming cannot reach.
    let dim = Style::new().effects(Effects::DIMMED);
    table.row(vec![Cell::plain("battery"), Cell::styled("", dim)]);
    table.row(vec![Cell::plain("clock"), Cell::styled("exited", dim)]);

    let (mut ui, transcript) = Ui::recording();
    ui.table(&table);

    for line in transcript.out().lines() {
        assert_eq!(line, line.trim_end(), "{line:?}");
    }
}

#[test]
fn a_size_reads_as_a_disk_is_sold() {
    // Bytes stay bytes: `1.0 kB` for a file you could read is worse than the
    // number, and rounding hides that a directory is nearly empty.
    assert_eq!(Paint::size(0), "0 B");
    assert_eq!(Paint::size(999), "999 B");

    // Decimal, not binary: `du -h` says GiB and disks are sold in GB. The
    // one a person compares this against is the vendor's.
    assert_eq!(Paint::size(1_000), "1.0 kB");
    assert_eq!(Paint::size(999_999_999_999_999), "1000 TB");

    // One decimal below ten, none above, so a column of sizes stays narrow
    // without losing the difference between 1.0 and 9.9 GB.
    assert_eq!(Paint::size(1_013_000_000), "1.0 GB");
    assert_eq!(Paint::size(894_000_000), "894 MB");
    assert_eq!(Paint::size(9_900_000_000), "9.9 GB");
    assert_eq!(Paint::size(12_000_000_000), "12 GB");
}
