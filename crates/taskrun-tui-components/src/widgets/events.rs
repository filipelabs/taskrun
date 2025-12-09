//! Events widget for displaying execution events.

use chrono::{DateTime, Utc};
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem};
use ratatui::Frame;

use crate::theme::Theme;

/// Information about an event.
#[derive(Debug, Clone)]
pub struct EventInfo {
    pub event_type: String,
    pub timestamp: DateTime<Utc>,
    pub details: Option<String>,
}

/// Widget for displaying execution events.
#[derive(Debug, Clone)]
pub struct EventsWidget<'a> {
    /// Events to display.
    events: &'a [EventInfo],
    /// Scroll offset.
    scroll: usize,
    /// Whether the widget is focused.
    focused: bool,
    /// Title override.
    title: Option<String>,
    /// Theme for styling.
    theme: Theme,
}

impl<'a> EventsWidget<'a> {
    /// Create a new events widget.
    pub fn new(events: &'a [EventInfo]) -> Self {
        Self {
            events,
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
        let total_events = self.events.len();

        // Clamp scroll offset
        let max_scroll = total_events.saturating_sub(visible_height);
        let scroll_offset = self.scroll.min(max_scroll);

        let items: Vec<ListItem> = self
            .events
            .iter()
            .skip(scroll_offset)
            .take(visible_height)
            .map(|event| {
                let timestamp = event.timestamp.format("%H:%M:%S");
                let event_style = Self::style_for_event(&event.event_type, &self.theme);

                let mut spans = vec![
                    Span::styled(format!("{} ", timestamp), self.theme.muted_style()),
                    Span::styled(&event.event_type, event_style),
                ];

                if let Some(ref details) = event.details {
                    spans.push(Span::raw(" -> "));
                    spans.push(Span::styled(details, Style::default().fg(Color::Gray)));
                }

                ListItem::new(Line::from(spans))
            })
            .collect();

        // Build title
        let title = self.title.unwrap_or_else(|| {
            if total_events > visible_height {
                format!(
                    " Events [{}-{}/{}] ",
                    scroll_offset + 1,
                    (scroll_offset + visible_height).min(total_events),
                    total_events
                )
            } else {
                format!(" Events [{} total] ", total_events)
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

    /// Get the appropriate style for an event type.
    fn style_for_event(event_type: &str, theme: &Theme) -> Style {
        if event_type.contains("Started") || event_type.contains("Completed") {
            theme.success_style()
        } else if event_type.contains("Failed") {
            theme.error_style()
        } else if event_type.contains("Tool") {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::White)
        }
    }
}
