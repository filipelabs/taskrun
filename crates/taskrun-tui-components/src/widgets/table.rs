//! Table widget for displaying data in rows and columns.

use ratatui::layout::{Constraint, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, Cell, Row, Table, TableState};
use ratatui::Frame;

use crate::theme::Theme;

/// A column definition for the table.
#[derive(Debug, Clone)]
pub struct TableColumn {
    /// Column header text.
    pub header: String,
    /// Column width constraint.
    pub width: Constraint,
}

impl TableColumn {
    /// Create a new column with a fixed width.
    pub fn new(header: impl Into<String>, width: u16) -> Self {
        Self {
            header: header.into(),
            width: Constraint::Length(width),
        }
    }

    /// Create a column with minimum width (flexible).
    pub fn flex(header: impl Into<String>, min_width: u16) -> Self {
        Self {
            header: header.into(),
            width: Constraint::Min(min_width),
        }
    }
}

/// A cell in a table row.
#[derive(Debug, Clone)]
pub struct TableCell {
    /// Cell content.
    pub content: String,
    /// Optional cell style.
    pub style: Option<Style>,
}

impl TableCell {
    /// Create a new cell with content.
    pub fn new(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            style: None,
        }
    }

    /// Set the cell style.
    pub fn style(mut self, style: Style) -> Self {
        self.style = Some(style);
        self
    }

    /// Set the cell color.
    pub fn color(mut self, color: Color) -> Self {
        self.style = Some(Style::default().fg(color));
        self
    }

    /// Create a success-styled cell.
    pub fn success(content: impl Into<String>) -> Self {
        Self::new(content).color(Color::Green)
    }

    /// Create a warning-styled cell.
    pub fn warning(content: impl Into<String>) -> Self {
        Self::new(content).color(Color::Yellow)
    }

    /// Create an error-styled cell.
    pub fn error(content: impl Into<String>) -> Self {
        Self::new(content).color(Color::Red)
    }

    /// Create a muted-styled cell.
    pub fn muted(content: impl Into<String>) -> Self {
        Self::new(content).color(Color::DarkGray)
    }

    /// Create a cyan-styled cell (for IDs, values).
    pub fn cyan(content: impl Into<String>) -> Self {
        Self::new(content).color(Color::Cyan)
    }
}

/// A row in the table.
#[derive(Debug, Clone)]
pub struct TableRow {
    /// Cells in this row.
    pub cells: Vec<TableCell>,
}

impl TableRow {
    /// Create a new row with cells.
    pub fn new(cells: Vec<TableCell>) -> Self {
        Self { cells }
    }
}

/// Table widget for displaying structured data.
#[derive(Debug)]
pub struct DataTable<'a> {
    /// Table title.
    title: Option<String>,
    /// Column definitions.
    columns: &'a [TableColumn],
    /// Table rows.
    rows: &'a [TableRow],
    /// Currently selected row index.
    selected: Option<usize>,
    /// Theme for styling.
    theme: Theme,
}

impl<'a> DataTable<'a> {
    /// Create a new table with columns and rows.
    pub fn new(columns: &'a [TableColumn], rows: &'a [TableRow]) -> Self {
        Self {
            title: None,
            columns,
            rows,
            selected: None,
            theme: Theme::default(),
        }
    }

    /// Set the table title.
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Set the selected row index.
    pub fn selected(mut self, index: usize) -> Self {
        self.selected = Some(index);
        self
    }

    /// Set the theme.
    pub fn theme(mut self, theme: Theme) -> Self {
        self.theme = theme;
        self
    }

    /// Render the table.
    pub fn render(self, frame: &mut Frame, area: Rect) {
        // Build header row
        let header = Row::new(
            self.columns
                .iter()
                .map(|c| Cell::from(c.header.clone()))
                .collect::<Vec<_>>(),
        )
        .style(Style::default().add_modifier(Modifier::BOLD))
        .height(1);

        // Build data rows
        let rows: Vec<Row> = self
            .rows
            .iter()
            .enumerate()
            .map(|(i, row)| {
                let cells: Vec<Cell> = row
                    .cells
                    .iter()
                    .map(|cell| {
                        let c = Cell::from(cell.content.clone());
                        if let Some(style) = cell.style {
                            c.style(style)
                        } else {
                            c
                        }
                    })
                    .collect();

                let row_style = if self.selected == Some(i) {
                    Style::default().bg(Color::DarkGray)
                } else {
                    Style::default()
                };

                Row::new(cells).style(row_style).height(1)
            })
            .collect();

        // Build widths
        let widths: Vec<Constraint> = self.columns.iter().map(|c| c.width).collect();

        // Build title
        let title = self
            .title
            .unwrap_or_else(|| format!(" {} items ", self.rows.len()));

        let table = Table::new(rows, widths)
            .header(header)
            .block(Block::default().title(title).borders(Borders::ALL))
            .row_highlight_style(Style::default().add_modifier(Modifier::REVERSED));

        // Render with state for selection
        let mut state = TableState::default();
        if let Some(selected) = self.selected {
            if !self.rows.is_empty() {
                state.select(Some(selected));
            }
        }

        frame.render_stateful_widget(table, area, &mut state);
    }
}
