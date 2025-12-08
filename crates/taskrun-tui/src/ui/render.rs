//! Main render function for the TUI.

use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Tabs};
use ratatui::Frame;

use crate::event::ConnectionState;
use crate::state::{UiState, View};

/// Render the entire UI.
pub fn render(frame: &mut Frame, state: &UiState) {
    let area = frame.area();

    // Create main layout: header, body, footer
    let [header_area, body_area, footer_area] = Layout::vertical([
        Constraint::Length(3),
        Constraint::Fill(1),
        Constraint::Length(1),
    ])
    .areas(area);

    // Render header with tabs
    render_header(frame, header_area, state);

    // Render body based on current view
    render_body(frame, body_area, state);

    // Render footer with status
    render_footer(frame, footer_area, state);
}

/// Render the header with navigation tabs.
fn render_header(frame: &mut Frame, area: ratatui::layout::Rect, state: &UiState) {
    let titles = vec!["[1] Workers", "[2] Tasks", "[3] Runs", "[4] Trace"];

    let selected = match state.current_view {
        View::Workers => 0,
        View::Tasks => 1,
        View::Runs => 2,
        View::Trace => 3,
    };

    let tabs = Tabs::new(titles)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" TaskRun TUI ")
                .title_style(
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
        )
        .select(selected)
        .style(Style::default().fg(Color::White))
        .highlight_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );

    frame.render_widget(tabs, area);
}

/// Render the main body content.
fn render_body(frame: &mut Frame, area: ratatui::layout::Rect, state: &UiState) {
    let content = match state.current_view {
        View::Workers => render_workers_view(state),
        View::Tasks => render_tasks_view(state),
        View::Runs => render_runs_placeholder(),
        View::Trace => render_trace_placeholder(),
    };

    frame.render_widget(content, area);
}

/// Render the footer with status message and connection indicator.
fn render_footer(frame: &mut Frame, area: ratatui::layout::Rect, state: &UiState) {
    // Connection indicator
    let connection_indicator = match &state.connection_state {
        ConnectionState::Connecting => Span::styled(
            "[ CONNECTING ] ",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        ConnectionState::Connected => Span::styled(
            "[ CONNECTED ] ",
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ),
        ConnectionState::Disconnected { retry_in } => Span::styled(
            format!("[ DISCONNECTED - {}s ] ", retry_in.as_secs()),
            Style::default()
                .fg(Color::Red)
                .add_modifier(Modifier::BOLD),
        ),
    };

    let status = state.status_message.as_deref().unwrap_or("Ready");

    let status_color = if state.last_error.is_some() {
        Color::Red
    } else {
        match &state.connection_state {
            ConnectionState::Connected => Color::Green,
            ConnectionState::Connecting => Color::Yellow,
            ConnectionState::Disconnected { .. } => Color::Red,
        }
    };

    let help = " q: quit | Tab: next | 1-4: view | r: refresh/reconnect ";

    let footer = Line::from(vec![
        connection_indicator,
        Span::styled(status, Style::default().fg(status_color)),
        Span::raw(" | "),
        Span::styled(help, Style::default().fg(Color::DarkGray)),
    ]);

    frame.render_widget(Paragraph::new(footer), area);
}

// View renderers

fn render_workers_view(state: &UiState) -> Paragraph<'static> {
    let mut lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            "  Workers View",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
    ];

    if state.workers.is_empty() {
        lines.push(Line::from("  No workers connected."));
    } else {
        lines.push(Line::from(Span::styled(
            format!("  {} worker(s) connected:", state.workers.len()),
            Style::default().fg(Color::Green),
        )));
        lines.push(Line::from(""));

        for worker in &state.workers {
            let status = match worker.status {
                0 => "UNKNOWN",
                1 => "IDLE",
                2 => "BUSY",
                3 => "OFFLINE",
                _ => "?",
            };
            lines.push(Line::from(format!("  - {} [{}]", worker.worker_id, status)));
        }
    }

    Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Workers ")
            .border_style(Style::default().fg(Color::Cyan)),
    )
}

fn render_tasks_view(state: &UiState) -> Paragraph<'static> {
    let mut lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            "  Tasks View",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
    ];

    if state.tasks.is_empty() {
        lines.push(Line::from("  No tasks created."));
    } else {
        lines.push(Line::from(Span::styled(
            format!("  {} task(s):", state.tasks.len()),
            Style::default().fg(Color::Green),
        )));
        lines.push(Line::from(""));

        for task in &state.tasks {
            let status = match task.status {
                0 => "PENDING",
                1 => "RUNNING",
                2 => "COMPLETED",
                3 => "FAILED",
                4 => "CANCELLED",
                _ => "?",
            };
            let short_id = if task.id.len() > 8 {
                &task.id[..8]
            } else {
                &task.id
            };
            lines.push(Line::from(format!(
                "  - {}... [{}] {}",
                short_id, status, task.agent_name
            )));
        }
    }

    Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Tasks ")
            .border_style(Style::default().fg(Color::Cyan)),
    )
}

fn render_runs_placeholder() -> Paragraph<'static> {
    let text = vec![
        Line::from(""),
        Line::from(Span::styled(
            "  Runs View",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from("  Select a task to view runs."),
        Line::from(""),
        Line::from(Span::styled(
            "  This view will show:",
            Style::default().fg(Color::Yellow),
        )),
        Line::from("  - Run ID, worker, status"),
        Line::from("  - Duration and model used"),
        Line::from("  - Error messages if failed"),
    ];

    Paragraph::new(text).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Runs ")
            .border_style(Style::default().fg(Color::Cyan)),
    )
}

fn render_trace_placeholder() -> Paragraph<'static> {
    let text = vec![
        Line::from(""),
        Line::from(Span::styled(
            "  Agent Trace View",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from("  Select a run to view trace."),
        Line::from(""),
        Line::from(Span::styled(
            "  This view will show:",
            Style::default().fg(Color::Yellow),
        )),
        Line::from("  - Execution timeline"),
        Line::from("  - Tool requests and results"),
        Line::from("  - Assistant messages"),
        Line::from("  - Model/session info"),
    ];

    Paragraph::new(text).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Agent Trace ")
            .border_style(Style::default().fg(Color::Cyan)),
    )
}
