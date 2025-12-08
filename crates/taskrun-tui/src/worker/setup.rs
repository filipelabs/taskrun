//! Setup screen for worker TUI configuration.

use ratatui::crossterm::event::KeyCode;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, ListState, Paragraph};
use ratatui::Frame;

/// Predefined agent options.
pub const AGENT_OPTIONS: &[(&str, &str)] = &[
    ("general", "General-purpose agent that executes any task"),
    ("support_triage", "Classifies and triages support tickets"),
];

/// Predefined model options.
pub const MODEL_OPTIONS: &[(&str, &str)] = &[
    ("claude-sonnet-4-5", "Claude Sonnet 4.5 - Fast and capable"),
    ("claude-opus-4-5", "Claude Opus 4.5 - Most powerful"),
    ("claude-haiku-4-5", "Claude Haiku 4.5 - Quick and efficient"),
];

/// Which field is currently selected in the setup form.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetupField {
    Agent,
    Model,
    Start,
}

impl SetupField {
    pub fn next(&self) -> Self {
        match self {
            SetupField::Agent => SetupField::Model,
            SetupField::Model => SetupField::Start,
            SetupField::Start => SetupField::Agent,
        }
    }

    pub fn prev(&self) -> Self {
        match self {
            SetupField::Agent => SetupField::Start,
            SetupField::Model => SetupField::Agent,
            SetupField::Start => SetupField::Model,
        }
    }
}

/// Setup screen state.
#[derive(Debug)]
pub struct SetupState {
    pub current_field: SetupField,
    pub agent_index: usize,
    pub model_index: usize,
    pub agent_list_state: ListState,
    pub model_list_state: ListState,
}

impl Default for SetupState {
    fn default() -> Self {
        let mut agent_list_state = ListState::default();
        agent_list_state.select(Some(0));
        let mut model_list_state = ListState::default();
        model_list_state.select(Some(0));

        Self {
            current_field: SetupField::Agent,
            agent_index: 0,
            model_index: 0,
            agent_list_state,
            model_list_state,
        }
    }
}

impl SetupState {
    /// Get the selected agent name.
    pub fn selected_agent(&self) -> &str {
        AGENT_OPTIONS[self.agent_index].0
    }

    /// Get the selected model name.
    pub fn selected_model(&self) -> &str {
        MODEL_OPTIONS[self.model_index].0
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
                        self.agent_list_state.select(Some(self.agent_index));
                    }
                }
                SetupField::Model => {
                    if self.model_index > 0 {
                        self.model_index -= 1;
                        self.model_list_state.select(Some(self.model_index));
                    }
                }
                SetupField::Start => {}
            },
            KeyCode::Right => match self.current_field {
                SetupField::Agent => {
                    if self.agent_index < AGENT_OPTIONS.len() - 1 {
                        self.agent_index += 1;
                        self.agent_list_state.select(Some(self.agent_index));
                    }
                }
                SetupField::Model => {
                    if self.model_index < MODEL_OPTIONS.len() - 1 {
                        self.model_index += 1;
                        self.model_list_state.select(Some(self.model_index));
                    }
                }
                SetupField::Start => {}
            },
            KeyCode::Enter => {
                if self.current_field == SetupField::Start {
                    return true; // Setup complete
                }
                // Move to next field on Enter
                self.current_field = self.current_field.next();
            }
            _ => {}
        }
        false
    }
}

/// Render the setup screen.
pub fn render_setup(frame: &mut Frame, state: &mut SetupState) {
    let area = frame.area();

    // Create a centered popup
    let popup_width = 60.min(area.width.saturating_sub(4));
    let popup_height = 18.min(area.height.saturating_sub(4));
    let popup_area = centered_rect(popup_width, popup_height, area);

    // Clear the background
    frame.render_widget(Clear, popup_area);

    // Main block
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Worker Setup ")
        .title_style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD));
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
            Constraint::Length(1), // Title
            Constraint::Length(1), // Spacer
            Constraint::Length(1), // Agent label
            Constraint::Length(3), // Agent selector
            Constraint::Length(1), // Model label
            Constraint::Length(3), // Model selector
            Constraint::Length(1), // Spacer
            Constraint::Length(3), // Start button
            Constraint::Min(0),    // Remaining
        ])
        .split(inner);

    // Title
    let title = Paragraph::new("Configure your worker settings:")
        .style(Style::default().fg(Color::White));
    frame.render_widget(title, chunks[0]);

    // Agent label
    let agent_style = if state.current_field == SetupField::Agent {
        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Gray)
    };
    let agent_label = Paragraph::new("Agent:").style(agent_style);
    frame.render_widget(agent_label, chunks[2]);

    // Agent selector
    render_selector(
        frame,
        chunks[3],
        AGENT_OPTIONS,
        state.agent_index,
        state.current_field == SetupField::Agent,
    );

    // Model label
    let model_style = if state.current_field == SetupField::Model {
        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Gray)
    };
    let model_label = Paragraph::new("Model:").style(model_style);
    frame.render_widget(model_label, chunks[4]);

    // Model selector
    render_selector(
        frame,
        chunks[5],
        MODEL_OPTIONS,
        state.model_index,
        state.current_field == SetupField::Model,
    );

    // Start button
    let start_style = if state.current_field == SetupField::Start {
        Style::default()
            .fg(Color::Black)
            .bg(Color::Green)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Green)
    };
    let start_text = if state.current_field == SetupField::Start {
        "[ Start Worker ]"
    } else {
        "  Start Worker  "
    };
    let start_button = Paragraph::new(start_text)
        .style(start_style)
        .alignment(ratatui::layout::Alignment::Center);
    frame.render_widget(start_button, chunks[7]);

    // Help text at bottom
    let help = Paragraph::new(Line::from(vec![
        Span::styled("[Tab/Arrows]", Style::default().fg(Color::Cyan)),
        Span::raw(" Navigate  "),
        Span::styled("[Left/Right]", Style::default().fg(Color::Cyan)),
        Span::raw(" Select  "),
        Span::styled("[Enter]", Style::default().fg(Color::Cyan)),
        Span::raw(" Confirm  "),
        Span::styled("[Esc]", Style::default().fg(Color::Cyan)),
        Span::raw(" Quit"),
    ]))
    .alignment(ratatui::layout::Alignment::Center);

    if chunks.len() > 8 && chunks[8].height > 0 {
        frame.render_widget(help, chunks[8]);
    }
}

/// Render a horizontal selector with options.
fn render_selector(
    frame: &mut Frame,
    area: Rect,
    options: &[(&str, &str)],
    selected: usize,
    is_focused: bool,
) {
    let border_style = if is_focused {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style);

    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Build the selector line
    let mut spans = Vec::new();

    // Left arrow
    if selected > 0 {
        spans.push(Span::styled(" < ", Style::default().fg(Color::Cyan)));
    } else {
        spans.push(Span::raw("   "));
    }

    // Selected option
    let (name, desc) = options[selected];
    spans.push(Span::styled(
        name,
        Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
    ));
    spans.push(Span::styled(
        format!(" - {}", desc),
        Style::default().fg(Color::DarkGray),
    ));

    // Right arrow
    if selected < options.len() - 1 {
        spans.push(Span::styled(" > ", Style::default().fg(Color::Cyan)));
    }

    let line = Line::from(spans);
    let text = Paragraph::new(line);
    frame.render_widget(text, inner);
}

/// Create a centered rectangle.
fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    Rect::new(x, y, width, height)
}
