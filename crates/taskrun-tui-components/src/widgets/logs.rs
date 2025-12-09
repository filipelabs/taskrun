//! Logs widget for displaying log messages.

use chrono::{DateTime, Utc};
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem};
use ratatui::Frame;

use crate::theme::Theme;

/// Log level for messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

impl LogLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            LogLevel::Debug => "DEBUG",
            LogLevel::Info => "INFO",
            LogLevel::Warn => "WARN",
            LogLevel::Error => "ERROR",
        }
    }
}

/// A single log entry.
#[derive(Debug, Clone)]
pub struct LogEntry {
    pub timestamp: DateTime<Utc>,
    pub level: LogLevel,
    pub message: String,
}

/// Widget for displaying log messages.
#[derive(Debug, Clone)]
pub struct LogsWidget<'a> {
    /// Log entries to display.
    entries: &'a [LogEntry],
    /// Scroll offset.
    scroll: usize,
    /// Whether the widget is focused.
    focused: bool,
    /// Title override.
    title: Option<String>,
    /// Theme for styling.
    theme: Theme,
}

impl<'a> LogsWidget<'a> {
    /// Create a new logs widget.
    pub fn new(entries: &'a [LogEntry]) -> Self {
        Self {
            entries,
            scroll: 0,
            focused: false,
            title: None,
            theme: Theme::default(),
        }
    }

    /// Set the scroll offset.
    pub fn scroll(mut self, offset: usize) -> Self {
        self.scroll = offset;
        self
    }

    /// Set whether the widget is focused.
    pub fn focused(mut self, focused: bool) -> Self {
        self.focused = focused;
        self
    }

    /// Set a custom title.
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Set the theme.
    pub fn theme(mut self, theme: Theme) -> Self {
        self.theme = theme;
        self
    }

    /// Render the widget.
    pub fn render(self, frame: &mut Frame, area: Rect) {
        let border_style = if self.focused {
            self.theme.focused_border()
        } else {
            self.theme.unfocused_border()
        };

        let visible_height = area.height.saturating_sub(2) as usize;
        let total_entries = self.entries.len();

        // Calculate scroll position
        let max_scroll = total_entries.saturating_sub(visible_height);
        let scroll_offset = self.scroll.min(max_scroll);

        let items: Vec<ListItem> = self
            .entries
            .iter()
            .skip(scroll_offset)
            .take(visible_height)
            .map(|entry| {
                let level_style = self.style_for_level(entry.level);
                let timestamp = entry.timestamp.format("%H:%M:%S").to_string();

                ListItem::new(Line::from(vec![
                    Span::styled(timestamp, self.theme.muted_style()),
                    Span::raw(" "),
                    Span::styled(format!("{:5}", entry.level.as_str()), level_style),
                    Span::raw(" "),
                    Span::raw(&entry.message),
                ]))
            })
            .collect();

        // Build title
        let title = self.title.unwrap_or_else(|| {
            if total_entries > visible_height {
                let start = scroll_offset + 1;
                let end = (scroll_offset + visible_height).min(total_entries);
                format!(" Logs [{}-{}/{}] ", start, end, total_entries)
            } else {
                format!(" Logs [{}] ", total_entries)
            }
        });

        let list = List::new(items).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(border_style)
                .title(title),
        );

        frame.render_widget(list, area);
    }

    /// Get the appropriate style for a log level.
    fn style_for_level(&self, level: LogLevel) -> Style {
        match level {
            LogLevel::Debug => self.theme.muted_style(),
            LogLevel::Info => Style::default().fg(ratatui::style::Color::Blue),
            LogLevel::Warn => self.theme.warning_style(),
            LogLevel::Error => self.theme.error_style(),
        }
    }
}
