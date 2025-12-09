//! Setup screen for worker TUI configuration.

use ratatui::crossterm::event::KeyCode;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

/// Predefined agent options.
pub const AGENT_OPTIONS: &[&str] = &["general", "support_triage"];

/// Predefined model options.
pub const MODEL_OPTIONS: &[&str] = &["sonnet", "opus", "haiku"];

/// Which field is currently selected in the setup form.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetupField {
    Agent,
    Model,
    SkipPermissions,
    Start,
}

impl SetupField {
    pub fn next(&self) -> Self {
        match self {
            SetupField::Agent => SetupField::Model,
            SetupField::Model => SetupField::SkipPermissions,
            SetupField::SkipPermissions => SetupField::Start,
            SetupField::Start => SetupField::Agent,
        }
    }

    pub fn prev(&self) -> Self {
        match self {
            SetupField::Agent => SetupField::Start,
            SetupField::Model => SetupField::Agent,
            SetupField::SkipPermissions => SetupField::Model,
            SetupField::Start => SetupField::SkipPermissions,
        }
    }
}

/// Setup screen state.
#[derive(Debug)]
pub struct SetupState {
    pub current_field: SetupField,
    pub agent_index: usize,
    pub model_index: usize,
    pub skip_permissions: bool,
}

impl Default for SetupState {
    fn default() -> Self {
        Self {
            current_field: SetupField::Agent,
            agent_index: 0,
            model_index: 0,
            skip_permissions: true, // Default to true for convenience
        }
    }
}

impl SetupState {
    /// Get the selected agent name.
    pub fn selected_agent(&self) -> &str {
        AGENT_OPTIONS[self.agent_index]
    }

    /// Get the selected model name.
    pub fn selected_model(&self) -> &str {
        MODEL_OPTIONS[self.model_index]
    }

    /// Handle a key press. Returns true if setup is complete (Enter on Start).
    pub fn handle_key(&mut self, code: KeyCode) -> bool {
        match code {
            KeyCode::Tab | KeyCode::Down => {
                self.current_field = self.current_field.next();
            }
            KeyCode::BackTab | KeyCode::Up => {
                self.current_field = self.current_field.prev();
            }
            KeyCode::Left => match self.current_field {
                SetupField::Agent => {
                    if self.agent_index > 0 {
                        self.agent_index -= 1;
                    }
                }
                SetupField::Model => {
                    if self.model_index > 0 {
                        self.model_index -= 1;
                    }
                }
                SetupField::SkipPermissions => {
                    self.skip_permissions = !self.skip_permissions;
                }
                SetupField::Start => {}
            },
            KeyCode::Right => match self.current_field {
                SetupField::Agent => {
                    if self.agent_index < AGENT_OPTIONS.len() - 1 {
                        self.agent_index += 1;
                    }
                }
                SetupField::Model => {
                    if self.model_index < MODEL_OPTIONS.len() - 1 {
                        self.model_index += 1;
                    }
                }
                SetupField::SkipPermissions => {
                    self.skip_permissions = !self.skip_permissions;
                }
                SetupField::Start => {}
            },
            KeyCode::Char(' ') => {
                if self.current_field == SetupField::SkipPermissions {
                    self.skip_permissions = !self.skip_permissions;
                }
            }
            KeyCode::Enter => {
                if self.current_field == SetupField::Start {
                    return true; // Setup complete
                }
                // Toggle on enter for checkbox
                if self.current_field == SetupField::SkipPermissions {
                    self.skip_permissions = !self.skip_permissions;
                } else {
                    // Move to next field on Enter
                    self.current_field = self.current_field.next();
                }
            }
            _ => {}
        }
        false
    }
}

/// Render the setup screen.
pub fn render_setup(frame: &mut Frame, state: &mut SetupState) {
    let area = frame.area();

    // Create a centered popup - more compact
    let popup_width = 50.min(area.width.saturating_sub(4));
    let popup_height = 12.min(area.height.saturating_sub(4));
    let popup_area = centered_rect(popup_width, popup_height, area);

    // Clear the background
    frame.render_widget(Clear, popup_area);

    // Main block
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Worker Setup ")
        .title_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );
    frame.render_widget(block, popup_area);

    // Inner area
    let inner = Rect {
        x: popup_area.x + 2,
        y: popup_area.y + 1,
        width: popup_area.width.saturating_sub(4),
        height: popup_area.height.saturating_sub(2),
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Agent row
            Constraint::Length(1), // Model row
            Constraint::Length(1), // Skip permissions row
            Constraint::Length(1), // Spacer
            Constraint::Length(1), // Start button
            Constraint::Length(1), // Spacer
            Constraint::Min(0),    // Help text
        ])
        .split(inner);

    // Agent row
    render_option_row(
        frame,
        chunks[0],
        "Agent",
        AGENT_OPTIONS,
        state.agent_index,
        state.current_field == SetupField::Agent,
    );

    // Model row
    render_option_row(
        frame,
        chunks[1],
        "Model",
        MODEL_OPTIONS,
        state.model_index,
        state.current_field == SetupField::Model,
    );

    // Skip permissions row
    render_toggle_row(
        frame,
        chunks[2],
        "Skip Permissions",
        state.skip_permissions,
        state.current_field == SetupField::SkipPermissions,
    );

    // Start button
    let start_focused = state.current_field == SetupField::Start;
    let start_style = if start_focused {
        Style::default()
            .fg(Color::Black)
            .bg(Color::Green)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Green)
    };
    let start_text = if start_focused {
        "▶ Start Worker"
    } else {
        "  Start Worker"
    };
    let start_button = Paragraph::new(start_text)
        .style(start_style)
        .alignment(ratatui::layout::Alignment::Center);
    frame.render_widget(start_button, chunks[4]);

    // Help text
    let help = Paragraph::new(Line::from(vec![
        Span::styled("↑↓", Style::default().fg(Color::Cyan)),
        Span::raw(" nav  "),
        Span::styled("←→", Style::default().fg(Color::Cyan)),
        Span::raw(" select  "),
        Span::styled("Enter", Style::default().fg(Color::Cyan)),
        Span::raw(" confirm  "),
        Span::styled("Esc", Style::default().fg(Color::Cyan)),
        Span::raw(" quit"),
    ]))
    .alignment(ratatui::layout::Alignment::Center)
    .style(Style::default().fg(Color::DarkGray));

    if chunks.len() > 6 && chunks[6].height > 0 {
        frame.render_widget(help, chunks[6]);
    }
}

/// Render a single-line option row with left/right selection.
fn render_option_row(
    frame: &mut Frame,
    area: Rect,
    label: &str,
    options: &[&str],
    selected: usize,
    is_focused: bool,
) {
    let label_style = if is_focused {
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Gray)
    };

    let value_style = if is_focused {
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::White)
    };

    let arrow_style = Style::default().fg(Color::Cyan);

    let mut spans = vec![Span::styled(format!("{:>16}: ", label), label_style)];

    // Left arrow
    if selected > 0 && is_focused {
        spans.push(Span::styled("◀ ", arrow_style));
    } else {
        spans.push(Span::raw("  "));
    }

    // Value
    spans.push(Span::styled(options[selected], value_style));

    // Right arrow
    if selected < options.len() - 1 && is_focused {
        spans.push(Span::styled(" ▶", arrow_style));
    }

    let line = Paragraph::new(Line::from(spans));
    frame.render_widget(line, area);
}

/// Render a toggle row with checkbox.
fn render_toggle_row(frame: &mut Frame, area: Rect, label: &str, value: bool, is_focused: bool) {
    let label_style = if is_focused {
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Gray)
    };

    let checkbox = if value {
        Span::styled(
            "[✓]",
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        )
    } else {
        Span::styled("[ ]", Style::default().fg(Color::DarkGray))
    };

    let hint = if is_focused {
        Span::styled(" (space to toggle)", Style::default().fg(Color::DarkGray))
    } else {
        Span::raw("")
    };

    let line = Paragraph::new(Line::from(vec![
        Span::styled(format!("{:>16}: ", label), label_style),
        checkbox,
        hint,
    ]));
    frame.render_widget(line, area);
}

/// Create a centered rectangle.
fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    Rect::new(x, y, width, height)
}
