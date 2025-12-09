//! Chat widget for displaying conversation messages.

use chrono::{DateTime, Utc};
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::theme::Theme;
use crate::utils::wrap_text_indented;

/// Role of a chat message participant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatRole {
    User,
    Assistant,
    System,
}

/// A single chat message.
#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub role: ChatRole,
    pub content: String,
    pub timestamp: DateTime<Utc>,
}

/// Configuration for the chat widget.
#[derive(Debug, Clone)]
pub struct ChatWidget<'a> {
    /// Messages to display.
    messages: &'a [ChatMessage],
    /// Current streaming output (if any).
    streaming_output: Option<&'a str>,
    /// Scroll offset (usize::MAX = auto-scroll to bottom).
    scroll: usize,
    /// Whether the widget is focused.
    focused: bool,
    /// Title override.
    title: Option<String>,
    /// Theme for styling.
    theme: Theme,
}

impl<'a> ChatWidget<'a> {
    /// Create a new chat widget.
    pub fn new(messages: &'a [ChatMessage]) -> Self {
        Self {
            messages,
            streaming_output: None,
            scroll: usize::MAX,
            focused: false,
            title: None,
            theme: Theme::default(),
        }
    }

    /// Set the streaming output to display.
    pub fn streaming(mut self, output: &'a str) -> Self {
        if !output.is_empty() {
            self.streaming_output = Some(output);
        }
        self
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
        let text_width = area.width.saturating_sub(2) as usize;

        // Build all message lines
        let mut all_lines: Vec<Line> = Vec::new();

        for msg in self.messages {
            let (prefix, style) = match msg.role {
                ChatRole::User => (
                    "You: ",
                    self.theme.user_style().add_modifier(Modifier::BOLD),
                ),
                ChatRole::Assistant => (
                    "AI: ",
                    self.theme.assistant_style().add_modifier(Modifier::BOLD),
                ),
                ChatRole::System => (
                    "System: ",
                    self.theme.system_style().add_modifier(Modifier::BOLD),
                ),
            };

            // Add message header
            all_lines.push(Line::from(vec![
                Span::styled(prefix, style),
                Span::styled(
                    msg.timestamp.format("%H:%M:%S").to_string(),
                    self.theme.muted_style(),
                ),
            ]));

            // Add message content with word wrapping
            for wrapped_line in wrap_text_indented(&msg.content, text_width, "  ") {
                all_lines.push(Line::from(Span::raw(wrapped_line)));
            }

            // Add blank line between messages
            all_lines.push(Line::from(""));
        }

        // If there's streaming output, show it
        if let Some(streaming) = self.streaming_output {
            all_lines.push(Line::from(vec![
                Span::styled(
                    "AI: ",
                    self.theme.assistant_style().add_modifier(Modifier::BOLD),
                ),
                Span::styled("(streaming...)", self.theme.muted_style()),
            ]));
            for wrapped_line in wrap_text_indented(streaming, text_width, "  ") {
                all_lines.push(Line::from(Span::raw(wrapped_line)));
            }
        }

        let total_lines = all_lines.len();

        // Calculate scroll position
        let max_scroll = total_lines.saturating_sub(visible_height);
        let scroll_offset = if self.scroll == usize::MAX {
            max_scroll
        } else {
            self.scroll.min(max_scroll)
        };

        let lines: Vec<Line> = all_lines
            .into_iter()
            .skip(scroll_offset)
            .take(visible_height)
            .collect();

        // Build title
        let first_line = scroll_offset + 1;
        let last_line = (scroll_offset + visible_height).min(total_lines);
        let title = self
            .title
            .unwrap_or_else(|| format!(" Chat [{}-{}/{}] ", first_line, last_line, total_lines));

        let paragraph = Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(border_style)
                .title(title),
        );

        frame.render_widget(paragraph, area);
    }
}
