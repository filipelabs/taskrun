//! Dialog overlays.

use ratatui::layout::{Alignment, Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use taskrun_tui_components::{centered_rect, ConfirmDialog};

use crate::state::ServerUiState;

/// Render the quit confirmation dialog.
pub fn render_quit_confirm(f: &mut Frame) {
    ConfirmDialog::new("Quit", "Are you sure you want to quit?").render(f);
}

/// Render the new task dialog.
pub fn render_new_task_dialog(f: &mut Frame, state: &ServerUiState) {
    let area = centered_rect(60, 12, f.area());

    f.render_widget(Clear, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(1), // Title
            Constraint::Length(1), // Spacing
            Constraint::Length(1), // Agent label
            Constraint::Length(1), // Agent input
            Constraint::Length(1), // Spacing
            Constraint::Length(1), // Input label
            Constraint::Length(1), // Input field
            Constraint::Length(1), // Spacing
            Constraint::Length(1), // Help
        ])
        .split(area);

    // Background block
    let block = Block::default()
        .title(" New Task ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));
    f.render_widget(block, area);

    // Agent label
    let agent_style = if state.new_task_field == 0 {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };
    let agent_label = Paragraph::new("Agent name:").style(agent_style);
    f.render_widget(agent_label, chunks[2]);

    // Agent input
    let agent_value = render_input_field(
        &state.new_task_agent,
        state.new_task_field == 0,
        state.new_task_cursor,
    );
    f.render_widget(agent_value, chunks[3]);

    // Input label
    let input_style = if state.new_task_field == 1 {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };
    let input_label = Paragraph::new("Input JSON:").style(input_style);
    f.render_widget(input_label, chunks[5]);

    // Input field
    let input_value = render_input_field(
        &state.new_task_input,
        state.new_task_field == 1,
        state.new_task_cursor,
    );
    f.render_widget(input_value, chunks[6]);

    // Help
    let help = Paragraph::new(Line::from(vec![
        Span::styled("Tab", Style::default().fg(Color::Yellow)),
        Span::raw(": Switch field  "),
        Span::styled("Enter", Style::default().fg(Color::Green)),
        Span::raw(": Submit  "),
        Span::styled("Esc", Style::default().fg(Color::Red)),
        Span::raw(": Cancel"),
    ]))
    .alignment(Alignment::Center);
    f.render_widget(help, chunks[8]);
}

fn render_input_field(value: &str, focused: bool, cursor: usize) -> Paragraph<'static> {
    let display_value = if focused {
        // Show cursor
        let char_count = value.chars().count();
        let cursor_pos = cursor.min(char_count);

        let before: String = value.chars().take(cursor_pos).collect();
        let after: String = value.chars().skip(cursor_pos).collect();

        format!("{}_{}  ", before, after)
    } else {
        format!("{}  ", value)
    };

    let style = if focused {
        Style::default().bg(Color::DarkGray)
    } else {
        Style::default()
    };

    Paragraph::new(display_value).style(style)
}

/// Render the cancel task confirmation dialog.
pub fn render_cancel_confirm(f: &mut Frame, state: &ServerUiState) {
    let task_id = state
        .get_selected_task()
        .map(|t| t.task_id.to_string()[..8].to_string())
        .unwrap_or_else(|| "?".to_string());

    ConfirmDialog::new("Cancel Task", &format!("Cancel task {}?", task_id))
        .secondary("This will stop any running executions.")
        .size(50, 9)
        .render(f);
}

/// Render the disconnect worker confirmation dialog.
pub fn render_disconnect_confirm(f: &mut Frame, state: &ServerUiState) {
    let worker_id = state
        .get_selected_worker()
        .map(|w| w.worker_id.to_string()[..8].to_string())
        .unwrap_or_else(|| "?".to_string());

    ConfirmDialog::new(
        "Disconnect Worker",
        &format!("Disconnect worker {}?", worker_id),
    )
    .secondary("Active runs will be reassigned.")
    .size(50, 9)
    .render(f);
}
