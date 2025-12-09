//! Run detail view widget combining chat, events, and input.

use chrono::{DateTime, Utc};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};
use ratatui::Frame;

use crate::theme::Theme;
use crate::utils::wrap_text_indented;

/// Status of a run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunStatus {
    Running,
    Completed,
    Failed,
}

/// Role in a chat message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageRole {
    User,
    Assistant,
}

/// A chat message in the run.
#[derive(Debug, Clone)]
pub struct RunMessage {
    pub role: MessageRole,
    pub content: String,
    pub timestamp: DateTime<Utc>,
}

/// An event in the run.
#[derive(Debug, Clone)]
pub struct RunEvent {
    pub event_type: String,
    pub timestamp: DateTime<Utc>,
    pub details: Option<String>,
}

/// Information about a run to display.
#[derive(Debug, Clone)]
pub struct RunInfo {
    pub run_id: String,
    pub task_id: String,
    pub agent: String,
    pub status: RunStatus,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub messages: Vec<RunMessage>,
    pub events: Vec<RunEvent>,
    pub current_output: String,
    pub queued_input: Option<String>,
}

/// Which pane is focused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DetailPane {
    #[default]
    Chat,
    Events,
    Input,
}

/// Run detail view combining header, chat, events, and input.
pub struct RunDetailView<'a> {
    /// Run information.
    run: &'a RunInfo,
    /// Which pane is focused.
    focused_pane: DetailPane,
    /// Chat scroll offset.
    chat_scroll: usize,
    /// Events scroll offset.
    events_scroll: usize,
    /// Current input text.
    input_text: &'a str,
    /// Input cursor position.
    input_cursor: usize,
    /// Theme for styling.
    theme: Theme,
}

impl<'a> RunDetailView<'a> {
    /// Create a new run detail view.
    pub fn new(run: &'a RunInfo) -> Self {
        Self {
            run,
            focused_pane: DetailPane::Chat,
            chat_scroll: usize::MAX,
            events_scroll: 0,
            input_text: "",
            input_cursor: 0,
            theme: Theme::default(),
        }
    }

    /// Set the focused pane.
    pub fn focused_pane(mut self, pane: DetailPane) -> Self {
        self.focused_pane = pane;
        self
    }

    /// Set the chat scroll offset.
    pub fn chat_scroll(mut self, scroll: usize) -> Self {
        self.chat_scroll = scroll;
        self
    }

    /// Set the events scroll offset.
    pub fn events_scroll(mut self, scroll: usize) -> Self {
        self.events_scroll = scroll;
        self
    }

    /// Set the input text and cursor.
    pub fn input(mut self, text: &'a str, cursor: usize) -> Self {
        self.input_text = text;
        self.input_cursor = cursor;
        self
    }

    /// Set the theme.
    pub fn theme(mut self, theme: Theme) -> Self {
        self.theme = theme;
        self
    }

    /// Render the view.
    pub fn render(self, frame: &mut Frame, area: Rect) {
        // Layout: header + chat/events split + input box
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Status header
                Constraint::Min(0),    // Chat + events
                Constraint::Length(3), // Input box
            ])
            .split(area);

        self.render_header(frame, chunks[0]);

        // Split chat and events
        let content_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(70), // Chat (wider)
                Constraint::Percentage(30), // Events
            ])
            .split(chunks[1]);

        self.render_chat(frame, content_chunks[0]);
        self.render_events(frame, content_chunks[1]);
        self.render_input(frame, chunks[2]);
    }

    /// Render the status header.
    fn render_header(&self, frame: &mut Frame, area: Rect) {
        let (status_str, status_color) = match self.run.status {
            RunStatus::Running => ("● Running", Color::Yellow),
            RunStatus::Completed => ("✓ Completed", Color::Green),
            RunStatus::Failed => ("✗ Failed", Color::Red),
        };

        let duration = if let Some(completed) = self.run.completed_at {
            let dur = completed.signed_duration_since(self.run.started_at);
            format!("{}s", dur.num_seconds())
        } else {
            let dur = chrono::Utc::now().signed_duration_since(self.run.started_at);
            format!("{}s", dur.num_seconds())
        };

        let header = Paragraph::new(Line::from(vec![
            Span::styled(status_str, Style::default().fg(status_color)),
            Span::raw(" | "),
            Span::raw("Agent: "),
            Span::styled(&self.run.agent, Style::default().fg(Color::Cyan)),
            Span::raw(" | "),
            Span::styled(duration, Style::default().fg(Color::DarkGray)),
            Span::raw(" | "),
            Span::styled(
                format!("{} messages", self.run.messages.len()),
                Style::default().fg(Color::DarkGray),
            ),
        ]))
        .block(Block::default().borders(Borders::ALL));

        frame.render_widget(header, area);
    }

    /// Render chat messages.
    fn render_chat(&self, frame: &mut Frame, area: Rect) {
        let is_focused = self.focused_pane == DetailPane::Chat;
        let border_style = if is_focused {
            self.theme.focused_border()
        } else {
            self.theme.unfocused_border()
        };

        let visible_height = area.height.saturating_sub(2) as usize;
        let text_width = area.width.saturating_sub(2) as usize;

        // Build all message lines
        let mut all_lines: Vec<Line> = Vec::new();

        for msg in &self.run.messages {
            let (prefix, style) = match msg.role {
                MessageRole::User => ("You: ", Style::default().fg(Color::Green)),
                MessageRole::Assistant => ("AI: ", Style::default().fg(Color::Cyan)),
            };

            // Add message header
            all_lines.push(Line::from(vec![
                Span::styled(prefix, style.add_modifier(Modifier::BOLD)),
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
        if !self.run.current_output.is_empty() {
            all_lines.push(Line::from(vec![
                Span::styled(
                    "AI: ",
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled("(streaming...)", self.theme.muted_style()),
            ]));
            for wrapped_line in wrap_text_indented(&self.run.current_output, text_width, "  ") {
                all_lines.push(Line::from(Span::raw(wrapped_line)));
            }
        }

        let total_lines = all_lines.len();

        // Calculate scroll position
        let max_scroll = total_lines.saturating_sub(visible_height);
        let scroll_offset = if self.chat_scroll == usize::MAX {
            max_scroll
        } else {
            self.chat_scroll.min(max_scroll)
        };

        let lines: Vec<Line> = all_lines
            .into_iter()
            .skip(scroll_offset)
            .take(visible_height)
            .collect();

        let first_line = scroll_offset + 1;
        let last_line = (scroll_offset + visible_height).min(total_lines);
        let title = format!(" Chat [{}-{}/{}] ", first_line, last_line, total_lines);

        let chat = Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(border_style)
                .title(title),
        );

        frame.render_widget(chat, area);
    }

    /// Render events pane.
    fn render_events(&self, frame: &mut Frame, area: Rect) {
        let is_focused = self.focused_pane == DetailPane::Events;
        let border_style = if is_focused {
            self.theme.focused_border()
        } else {
            self.theme.unfocused_border()
        };

        let visible_height = area.height.saturating_sub(2) as usize;
        let total_events = self.run.events.len();

        // Clamp scroll offset
        let max_scroll = total_events.saturating_sub(visible_height);
        let scroll_offset = self.events_scroll.min(max_scroll);

        let items: Vec<ListItem> = self
            .run
            .events
            .iter()
            .skip(scroll_offset)
            .take(visible_height)
            .map(|event| {
                let timestamp = event.timestamp.format("%H:%M:%S");
                let event_style = match event.event_type.as_str() {
                    s if s.contains("Started") => Style::default().fg(Color::Green),
                    s if s.contains("Completed") => Style::default().fg(Color::Green),
                    s if s.contains("Failed") => Style::default().fg(Color::Red),
                    s if s.contains("Tool") => Style::default().fg(Color::Cyan),
                    _ => Style::default().fg(Color::White),
                };

                let mut spans = vec![
                    Span::styled(format!("{} ", timestamp), self.theme.muted_style()),
                    Span::styled(&event.event_type, event_style),
                ];

                if let Some(ref details) = event.details {
                    spans.push(Span::raw(" → "));
                    spans.push(Span::styled(details, Style::default().fg(Color::Gray)));
                }

                ListItem::new(Line::from(spans))
            })
            .collect();

        let title = if total_events > visible_height {
            format!(
                " Events [{}-{}/{}] ",
                scroll_offset + 1,
                (scroll_offset + visible_height).min(total_events),
                total_events
            )
        } else {
            format!(" Events [{} total] ", total_events)
        };

        let list = List::new(items).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(border_style)
                .title(title),
        );

        frame.render_widget(list, area);
    }

    /// Render input box.
    fn render_input(&self, frame: &mut Frame, area: Rect) {
        let is_focused = self.focused_pane == DetailPane::Input;
        let border_style = if is_focused {
            self.theme.focused_border()
        } else {
            self.theme.unfocused_border()
        };

        // Determine title and content based on state
        let (title, content, text_style) = if let Some(ref queued) = self.run.queued_input {
            (
                " Queued (will send when run completes) ",
                queued.clone(),
                Style::default().fg(Color::Yellow),
            )
        } else if self.run.status == RunStatus::Running {
            (
                " Type message (queued until run completes) ",
                self.input_text.to_string(),
                Style::default().fg(Color::White),
            )
        } else {
            (
                " Type message (Enter to send) ",
                self.input_text.to_string(),
                Style::default().fg(Color::White),
            )
        };

        // Add cursor if focused and no queued message
        let display_text = if is_focused && self.run.queued_input.is_none() {
            let chars: Vec<char> = self.input_text.chars().collect();
            let cursor_pos = self.input_cursor.min(chars.len());
            let before: String = chars[..cursor_pos].iter().collect();
            let after: String = chars[cursor_pos..].iter().collect();
            format!("{}│{}", before, after)
        } else {
            content
        };

        let input = Paragraph::new(display_text).style(text_style).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(border_style)
                .title(title),
        );

        frame.render_widget(input, area);
    }
}
