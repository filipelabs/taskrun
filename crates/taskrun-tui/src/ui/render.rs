//! Main render function for the TUI.

use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Tabs};
use ratatui::Frame;

use crate::app::{App, View};

/// Render the entire UI.
pub fn render(frame: &mut Frame, app: &App) {
    let area = frame.area();

    // Create main layout: header, body, footer
    let [header_area, body_area, footer_area] = Layout::vertical([
        Constraint::Length(3),
        Constraint::Fill(1),
        Constraint::Length(1),
    ])
    .areas(area);

    // Render header with tabs
    render_header(frame, header_area, app);

    // Render body based on current view
    render_body(frame, body_area, app);

    // Render footer with status
    render_footer(frame, footer_area, app);
}

/// Render the header with navigation tabs.
fn render_header(frame: &mut Frame, area: ratatui::layout::Rect, app: &App) {
    let titles = vec!["[1] Workers", "[2] Tasks", "[3] Runs", "[4] Trace"];

    let selected = match app.current_view {
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
                .title_style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
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
fn render_body(frame: &mut Frame, area: ratatui::layout::Rect, app: &App) {
    let content = match app.current_view {
        View::Workers => render_workers_placeholder(),
        View::Tasks => render_tasks_placeholder(),
        View::Runs => render_runs_placeholder(),
        View::Trace => render_trace_placeholder(),
    };

    frame.render_widget(content, area);
}

/// Render the footer with status message.
fn render_footer(frame: &mut Frame, area: ratatui::layout::Rect, app: &App) {
    let status = app
        .status_message
        .as_deref()
        .unwrap_or("Ready");

    let help = " q: quit | Tab: next view | 1-4: switch view ";

    let footer = Line::from(vec![
        Span::styled(status, Style::default().fg(Color::Green)),
        Span::raw(" | "),
        Span::styled(help, Style::default().fg(Color::DarkGray)),
    ]);

    frame.render_widget(Paragraph::new(footer), area);
}

// Placeholder renderers - will be replaced with real data in future issues

fn render_workers_placeholder() -> Paragraph<'static> {
    let text = vec![
        Line::from(""),
        Line::from(Span::styled(
            "  Workers View",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from("  No workers connected yet."),
        Line::from(""),
        Line::from(Span::styled(
            "  This view will show:",
            Style::default().fg(Color::Yellow),
        )),
        Line::from("  - Worker ID, status, active runs"),
        Line::from("  - Supported agents and models"),
        Line::from("  - Connection time and last heartbeat"),
    ];

    Paragraph::new(text).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Workers ")
            .border_style(Style::default().fg(Color::Cyan)),
    )
}

fn render_tasks_placeholder() -> Paragraph<'static> {
    let text = vec![
        Line::from(""),
        Line::from(Span::styled(
            "  Tasks View",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from("  No tasks created yet."),
        Line::from(""),
        Line::from(Span::styled(
            "  This view will show:",
            Style::default().fg(Color::Yellow),
        )),
        Line::from("  - Task ID, agent, status"),
        Line::from("  - Created/updated timestamps"),
        Line::from("  - Number of runs per task"),
    ];

    Paragraph::new(text).block(
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
