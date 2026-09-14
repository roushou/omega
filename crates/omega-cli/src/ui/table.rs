//! Align terminal columns using visible text width, excluding ANSI escape sequences.

use std::fmt::Display;

use anstyle::Style;

/// Which edge of its column a cell sits against.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    Left,
    Right,
}

/// One cell: what it says, and how it is dressed.
#[derive(Debug, Clone)]
pub struct Cell {
    text: String,
    style: Style,
}

impl Cell {
    pub fn plain(text: impl Display) -> Self {
        Self {
            text: text.to_string(),
            style: Style::new(),
        }
    }

    pub fn styled(text: impl Display, style: Style) -> Self {
        Self {
            text: text.to_string(),
            style,
        }
    }

    fn width(&self) -> usize {
        self.text.chars().count()
    }

    fn render(&self, width: usize, align: Align) -> String {
        let padding = " ".repeat(width.saturating_sub(self.width()));

        // Do not emit ANSI styling for empty cells.
        if self.text.is_empty() {
            return padding;
        }

        let Self { text, style } = self;
        match align {
            Align::Left => format!("{style}{text}{style:#}{padding}"),
            Align::Right => format!("{padding}{style}{text}{style:#}"),
        }
    }
}

/// A column: its heading and which way its cells align.
#[derive(Debug, Clone)]
pub struct Column {
    heading: &'static str,
    align: Align,
}

impl Column {
    pub fn left(heading: &'static str) -> Self {
        Self {
            heading,
            align: Align::Left,
        }
    }

    pub fn right(heading: &'static str) -> Self {
        Self {
            heading,
            align: Align::Right,
        }
    }
}

/// Rows under headings, rendered once every width is known.
#[derive(Debug)]
pub struct Table {
    columns: Vec<Column>,
    rows: Vec<Vec<Cell>>,
}

impl Table {
    /// The gap between columns: two spaces, so a column boundary is visible
    /// without a rule drawn down it.
    const GAP: &'static str = "  ";

    pub fn new(columns: Vec<Column>) -> Self {
        Self {
            columns,
            rows: Vec::new(),
        }
    }

    pub fn row(&mut self, cells: Vec<Cell>) {
        self.rows.push(cells);
    }

    /// Drop a column nothing filled in. An empty `DETAIL` heading over an
    /// empty column is a question the reader has to answer for themselves.
    pub fn drop_empty(&mut self, column: usize) {
        let empty = self
            .rows
            .iter()
            .all(|row| row.get(column).is_none_or(|cell| cell.text.is_empty()));

        if empty && column < self.columns.len() {
            self.columns.remove(column);
            for row in &mut self.rows {
                if column < row.len() {
                    row.remove(column);
                }
            }
        }
    }

    /// Every line, headings first, each already padded to width.
    pub fn render(&self, heading: Style) -> Vec<String> {
        let widths = self.widths();

        let mut lines = vec![
            self.line(
                &self
                    .columns
                    .iter()
                    .map(|column| Cell::styled(column.heading, heading))
                    .collect::<Vec<_>>(),
                &widths,
            ),
        ];

        lines.extend(self.rows.iter().map(|row| self.line(row, &widths)));
        lines
    }

    fn widths(&self) -> Vec<usize> {
        self.columns
            .iter()
            .enumerate()
            .map(|(index, column)| {
                self.rows
                    .iter()
                    .filter_map(|row| row.get(index))
                    .map(Cell::width)
                    .chain(std::iter::once(column.heading.chars().count()))
                    .max()
                    .unwrap_or_default()
            })
            .collect()
    }

    /// One row, padded and then trimmed: trailing spaces are invisible until
    /// they are selected, copied, or diffed.
    fn line(&self, cells: &[Cell], widths: &[usize]) -> String {
        let rendered: Vec<String> = self
            .columns
            .iter()
            .enumerate()
            .map(|(index, column)| {
                cells
                    .get(index)
                    .cloned()
                    .unwrap_or_else(|| Cell::plain(""))
                    .render(widths[index], column.align)
            })
            .collect();

        rendered.join(Self::GAP).trim_end().to_string()
    }
}
