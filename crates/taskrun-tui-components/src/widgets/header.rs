//! Header widget for TUI applications.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Tabs};
use ratatui::Frame;

use crate::theme::Theme;

/// Status indicator for the header.
#[derive(Debug, Clone)]
pub struct StatusIndicator {
    pub label: String,
    pub color: Color,
}

impl StatusIndicator {
    pub fn new(label: impl Into<String>, color: Color) -> Self {
        Self {
            label: label.into(),
            color,
        }
    }

    pub fn success(label: impl Into<String>) -> Self {
        Self::new(label, Color::Green)
    }

    pub fn warning(label: impl Into<String>) -> Self {
        Self::new(label, Color::Yellow)
    }

    pub fn error(label: impl Into<String>) -> Self {
        Self::new(label, Color::Red)
    }
}

/// A stat to display in the header.
#[derive(Debug, Clone)]
pub struct HeaderStat {
    pub label: String,
    pub value: String,
    pub color: Color,
}

impl HeaderStat {
    pub fn new(label: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            value: value.into(),
            color: Color::Cyan,
        }
    }

    pub fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }
}

/// Header widget with title, status, tabs, and stats.
#[derive(Debug, Clone)]
pub struct Header<'a> {
    /// Application title.
    title: &'a str,
    /// Status indicator.
    status: Option<StatusIndicator>,
    /// Tab labels.
    tabs: Vec<&'a str>,
    /// Selected tab index.
    selected_tab: usize,
    /// Stats to display on the right side.
    stats: Vec<HeaderStat>,
    /// Theme for styling.
    theme: Theme,
}

impl<'a> Header<'a> {
    /// Create a new header with a title.
    pub fn new(title: &'a str) -> Self {
        Self {
            title,
            status: None,
            tabs: Vec::new(),
            selected_tab: 0,
            stats: Vec::new(),
            theme: Theme::default(),
        }
    }

    /// Set the status indicator.
    pub fn status(mut self, status: StatusIndicator) -> Self {
        self.status = Some(status);
        self
    }

    /// Set the tabs.
    pub fn tabs(mut self, tabs: Vec<&'a str>, selected: usize) -> Self {
        self.tabs = tabs;
        self.selected_tab = selected;
        self
    }

    /// Add a stat to display.
    pub fn stat(mut self, stat: HeaderStat) -> Self {
        self.stats.push(stat);
        self
    }

    /// Add multiple stats.
    pub fn stats(mut self, stats: Vec<HeaderStat>) -> Self {
        self.stats.extend(stats);
        self
    }

    /// Set the theme.
    pub fn theme(mut self, theme: Theme) -> Self {
        self.theme = theme;
        self
    }

    /// Render the header.
    pub fn render(self, frame: &mut Frame, area: Rect) {
        // Split into left (title + tabs) and right (stats)
        let has_stats = !self.stats.is_empty();
        let constraints = if has_stats {
            vec![Constraint::Min(40), Constraint::Length(50)]
        } else {
            vec![Constraint::Min(0)]
        };

        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(constraints)
            .split(area);

        // Build title with status
        let mut title_spans = vec![Span::raw(format!(" {} ", self.title))];
        if let Some(status) = &self.status {
            title_spans.push(Span::styled(
                format!("[{}]", status.label),
                Style::default().fg(status.color),
            ));
            title_spans.push(Span::raw(" "));
        }

        // Build tabs
        let tab_titles: Vec<Line> = self
            .tabs
            .iter()
            .enumerate()
            .map(|(i, name)| {
                let style = if i == self.selected_tab {
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };
                Line::from(Span::styled(format!(" {} {} ", i + 1, name), style))
            })
            .collect();

        let tabs_widget = Tabs::new(tab_titles)
            .block(Block::default().title(title_spans).borders(Borders::ALL))
            .select(self.selected_tab)
            .divider("|");

        frame.render_widget(tabs_widget, chunks[0]);

        // Render stats if present
        if has_stats && chunks.len() > 1 {
            let mut stat_spans = vec![Span::raw(" ")];

            for (i, stat) in self.stats.iter().enumerate() {
                if i > 0 {
                    stat_spans.push(Span::raw(" | "));
                }
                stat_spans.push(Span::raw(format!("{}: ", stat.label)));
                stat_spans.push(Span::styled(&stat.value, Style::default().fg(stat.color)));
            }

            stat_spans.push(Span::raw(" "));

            let stats_widget = Paragraph::new(Line::from(stat_spans))
                .block(Block::default().borders(Borders::ALL));

            frame.render_widget(stats_widget, chunks[1]);
        }
    }
}
