//! Main render function that dispatches to view renderers.

use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::Frame;

use taskrun_tui_components::{Footer, Header, HeaderStat, StatusIndicator};

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

fn render_header(f: &mut Frame, state: &ServerUiState, area: ratatui::layout::Rect) {
    let status = match state.server_status {
        ServerStatus::Starting => StatusIndicator::warning("Starting..."),
        ServerStatus::Running => StatusIndicator::success("Running"),
        ServerStatus::Error => StatusIndicator::error("Error"),
    };

    let tabs: Vec<&str> = ServerView::all().iter().map(|v| v.name()).collect();
    let selected = ServerView::all()
        .iter()
        .position(|v| *v == state.current_view)
        .unwrap_or(0);

    let uptime = state.uptime();
    let uptime_str = format!(
        "{:02}:{:02}:{:02}",
        uptime.as_secs() / 3600,
        (uptime.as_secs() % 3600) / 60,
        uptime.as_secs() % 60
    );

    Header::new("TaskRun Server")
        .status(status)
        .tabs(tabs, selected)
        .stats(vec![
            HeaderStat::new("Workers", state.workers.len().to_string()),
            HeaderStat::new("Tasks", state.total_tasks.to_string()),
            HeaderStat::new("Done", state.completed_tasks.to_string())
                .color(ratatui::style::Color::Green),
            HeaderStat::new("Failed", state.failed_tasks.to_string())
                .color(ratatui::style::Color::Red),
            HeaderStat::new("Up", uptime_str),
        ])
        .render(f, area);
}

fn render_main_content(f: &mut Frame, state: &ServerUiState, area: ratatui::layout::Rect) {
    match state.current_view {
        ServerView::Workers => render_workers_view(f, state, area),
        ServerView::Tasks => render_tasks_view(f, state, area),
        ServerView::Logs => render_logs_view(f, state, area),
        ServerView::RunDetail => render_run_detail_view(f, state, area),
    }
}

fn render_footer(f: &mut Frame, state: &ServerUiState, area: ratatui::layout::Rect) {
    let help_text = match state.current_view {
        ServerView::Workers => "j/k: Navigate | d: Disconnect | Tab: Next view | q: Quit",
        ServerView::Tasks => {
            "j/k: Navigate | n: New task | c: Cancel | Enter: Details | Tab: Next view | q: Quit"
        }
        ServerView::Logs => "j/k: Scroll | g/G: Top/Bottom | Tab: Next view | q: Quit",
        ServerView::RunDetail => "j/k: Scroll | g/G: Top/Bottom | Esc: Back | q: Quit",
    };

    Footer::new(help_text).render(f, area);
}
