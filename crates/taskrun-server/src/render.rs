//! Main render function that dispatches to view renderers.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Tabs};
use ratatui::Frame;

use crate::state::{ServerStatus, ServerUiState, ServerView};
use crate::views::dialogs::{
    render_cancel_confirm, render_disconnect_confirm, render_new_task_dialog, render_quit_confirm,
};
use crate::views::{
    render_logs_view, render_run_detail_view, render_tasks_view, render_workers_view,
};

/// Main render function.
pub fn render(f: &mut Frame, state: &ServerUiState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header
            Constraint::Min(0),    // Main content
            Constraint::Length(1), // Footer
        ])
        .split(f.area());

    render_header(f, state, chunks[0]);
    render_main_content(f, state, chunks[1]);
    render_footer(f, state, chunks[2]);

    // Render dialogs on top
    if state.show_quit_confirm {
        render_quit_confirm(f);
    }
    if state.show_new_task_dialog {
        render_new_task_dialog(f, state);
    }
    if state.show_cancel_confirm {
        render_cancel_confirm(f, state);
    }
    if state.show_disconnect_confirm {
        render_disconnect_confirm(f, state);
    }
}

fn render_header(f: &mut Frame, state: &ServerUiState, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Min(40),    // Title + status
            Constraint::Length(50), // Stats
        ])
        .split(area);

    // Title and tabs
    let status_color = match state.server_status {
        ServerStatus::Starting => Color::Yellow,
        ServerStatus::Running => Color::Green,
        ServerStatus::Error => Color::Red,
    };

    let status_text = match state.server_status {
        ServerStatus::Starting => "Starting...",
        ServerStatus::Running => "Running",
        ServerStatus::Error => "Error",
    };

    let titles: Vec<Line> = ServerView::all()
        .iter()
        .enumerate()
        .map(|(i, v)| {
            let style = if *v == state.current_view {
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            Line::from(Span::styled(format!(" {} {} ", i + 1, v.name()), style))
        })
        .collect();

    let selected = ServerView::all()
        .iter()
        .position(|v| *v == state.current_view)
        .unwrap_or(0);

    let tabs = Tabs::new(titles)
        .block(
            Block::default()
                .title(vec![
                    Span::raw(" TaskRun Server "),
                    Span::styled(
                        format!("[{}]", status_text),
                        Style::default().fg(status_color),
                    ),
                    Span::raw(" "),
                ])
                .borders(Borders::ALL),
        )
        .select(selected)
        .divider("|");

    f.render_widget(tabs, chunks[0]);

    // Stats
    let uptime = state.uptime();
    let uptime_str = format!(
        "{:02}:{:02}:{:02}",
        uptime.as_secs() / 3600,
        (uptime.as_secs() % 3600) / 60,
        uptime.as_secs() % 60
    );

    let stats = Paragraph::new(Line::from(vec![
        Span::raw(" Workers: "),
        Span::styled(
            format!("{}", state.workers.len()),
            Style::default().fg(Color::Cyan),
        ),
        Span::raw(" | Tasks: "),
        Span::styled(
            format!("{}", state.total_tasks),
            Style::default().fg(Color::Cyan),
        ),
        Span::raw(" | "),
        Span::styled(
            format!("{}", state.completed_tasks),
            Style::default().fg(Color::Green),
        ),
        Span::raw("/"),
        Span::styled(
            format!("{}", state.failed_tasks),
            Style::default().fg(Color::Red),
        ),
        Span::raw(" | Up: "),
        Span::styled(uptime_str, Style::default().fg(Color::Cyan)),
        Span::raw(" "),
    ]))
    .block(Block::default().borders(Borders::ALL));

    f.render_widget(stats, chunks[1]);
}

fn render_main_content(f: &mut Frame, state: &ServerUiState, area: Rect) {
    match state.current_view {
        ServerView::Workers => render_workers_view(f, state, area),
        ServerView::Tasks => render_tasks_view(f, state, area),
        ServerView::Logs => render_logs_view(f, state, area),
        ServerView::RunDetail => render_run_detail_view(f, state, area),
    }
}

fn render_footer(f: &mut Frame, state: &ServerUiState, area: Rect) {
    let help_text = match state.current_view {
        ServerView::Workers => "j/k: Navigate | d: Disconnect | Tab: Next view | q: Quit",
        ServerView::Tasks => {
            "j/k: Navigate | n: New task | c: Cancel | Enter: Details | Tab: Next view | q: Quit"
        }
        ServerView::Logs => "j/k: Scroll | g/G: Top/Bottom | Tab: Next view | q: Quit",
        ServerView::RunDetail => "j/k: Scroll | g/G: Top/Bottom | Esc: Back | q: Quit",
    };

    let footer = Paragraph::new(help_text).style(Style::default().fg(Color::DarkGray));

    f.render_widget(footer, area);
}
