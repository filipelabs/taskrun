//! UI rendering for the worker TUI.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use taskrun_tui_components::{
    ConfirmDialog, DataTable, DetailPane as SharedDetailPane, Footer, Header, HeaderStat,
    InputDialog, LogsWidget, MessageRole, RunDetailInfo, RunDetailStatus, RunDetailView, RunEvent,
    RunMessage, StatusIndicator, TableCell, TableColumn, TableRow,
};

use super::state::{
    ChatRole, ConnectionState, DetailPane, RunInfo, RunStatus, WorkerUiState, WorkerView,
};

/// Main render function for the worker TUI.
pub fn render(frame: &mut Frame, state: &WorkerUiState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header
            Constraint::Min(0),    // Main content
            Constraint::Length(1), // Footer
        ])
        .split(frame.area());

    render_header(frame, chunks[0], state);
    render_main_content(frame, chunks[1], state);
    render_footer(frame, chunks[2], state);

    // Render dialogs on top
    if state.show_quit_confirm {
        render_quit_confirm(frame);
    }
    if state.show_new_run_dialog {
        render_new_run_dialog(frame, state);
    }
}

/// Render the header with tabs and stats.
fn render_header(frame: &mut Frame, area: Rect, state: &WorkerUiState) {
    let status = match &state.connection_state {
        ConnectionState::Connecting => StatusIndicator::warning("Connecting..."),
        ConnectionState::Connected => StatusIndicator::success("Connected"),
        ConnectionState::Disconnected { retry_in } => {
            StatusIndicator::error(format!("Retry {}s", retry_in.as_secs()))
        }
    };

    let tabs: Vec<&str> = WorkerView::all().iter().map(|v| v.name()).collect();
    let selected = WorkerView::all()
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

    Header::new("TaskRun Worker")
        .status(status)
        .tabs(tabs, selected)
        .stats(vec![
            HeaderStat::new("Agent", state.config.agent_name.clone()),
            HeaderStat::new(
                "Runs",
                format!(
                    "{}/{}",
                    state.active_runs.len(),
                    state.config.max_concurrent_runs
                ),
            ),
            HeaderStat::new("Done", state.stats.successful_runs.to_string()).color(Color::Green),
            HeaderStat::new("Failed", state.stats.failed_runs.to_string()).color(Color::Red),
            HeaderStat::new("Up", uptime_str),
        ])
        .render(frame, area);
}

/// Render the main content area based on current view.
fn render_main_content(frame: &mut Frame, area: Rect, state: &WorkerUiState) {
    match state.current_view {
        WorkerView::Status => render_status_view(frame, area, state),
        WorkerView::Runs => render_runs_view(frame, area, state),
        WorkerView::RunDetail => render_run_detail_view(frame, area, state),
        WorkerView::Logs => render_logs_view(frame, area, state),
        WorkerView::Config => render_config_view(frame, area, state),
    }
}

/// Render the footer with help text.
fn render_footer(frame: &mut Frame, area: Rect, state: &WorkerUiState) {
    let help_text = match state.current_view {
        WorkerView::Status => "Tab: Next view | q: Quit",
        WorkerView::Runs => "j/k: Navigate | n: New | Enter: Details | Tab: Next view | q: Quit",
        WorkerView::RunDetail => "j/k: Scroll | Tab: Switch pane | g/G: Top/Bottom | Esc: Back",
        WorkerView::Logs => "j/k: Scroll | g/G: Top/Bottom | Tab: Next view | q: Quit",
        WorkerView::Config => "Tab: Next view | q: Quit",
    };

    Footer::new(help_text).render(frame, area);
}

/// Render the status view.
fn render_status_view(frame: &mut Frame, area: Rect, state: &WorkerUiState) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    // Worker info (left)
    let (provider, model) = state.config.parse_model();

    let info_lines = vec![
        Line::from(vec![
            Span::styled("Worker ID:   ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                &state.worker_id[..8.min(state.worker_id.len())],
                Style::default().fg(Color::Cyan),
            ),
        ]),
        Line::from(vec![
            Span::styled("Agent:       ", Style::default().fg(Color::DarkGray)),
            Span::styled(&state.config.agent_name, Style::default().fg(Color::Cyan)),
        ]),
        Line::from(vec![
            Span::styled("Model:       ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("{}/{}", provider, model),
                Style::default().fg(Color::Cyan),
            ),
        ]),
        Line::from(vec![
            Span::styled("Endpoint:    ", Style::default().fg(Color::DarkGray)),
            Span::styled(&state.config.endpoint, Style::default().fg(Color::Cyan)),
        ]),
        Line::from(vec![
            Span::styled("Working Dir: ", Style::default().fg(Color::DarkGray)),
            Span::styled(&state.config.working_dir, Style::default().fg(Color::Cyan)),
        ]),
    ];

    let info = Paragraph::new(info_lines).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Worker Info "),
    );
    frame.render_widget(info, chunks[0]);

    // Stats (right)
    let success_rate = if state.stats.total_runs > 0 {
        format!(
            "{:.1}%",
            (state.stats.successful_runs as f64 / state.stats.total_runs as f64) * 100.0
        )
    } else {
        "N/A".to_string()
    };

    let stats_lines = vec![
        Line::from(vec![
            Span::styled("Total Runs:   ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                state.stats.total_runs.to_string(),
                Style::default().fg(Color::Cyan),
            ),
        ]),
        Line::from(vec![
            Span::styled("Successful:   ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                state.stats.successful_runs.to_string(),
                Style::default().fg(Color::Green),
            ),
        ]),
        Line::from(vec![
            Span::styled("Failed:       ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                state.stats.failed_runs.to_string(),
                if state.stats.failed_runs > 0 {
                    Style::default().fg(Color::Red)
                } else {
                    Style::default().fg(Color::DarkGray)
                },
            ),
        ]),
        Line::from(vec![
            Span::styled("Success Rate: ", Style::default().fg(Color::DarkGray)),
            Span::styled(success_rate, Style::default().fg(Color::Cyan)),
        ]),
    ];

    let stats = Paragraph::new(stats_lines)
        .block(Block::default().borders(Borders::ALL).title(" Statistics "));
    frame.render_widget(stats, chunks[1]);
}

/// Render the runs view using shared table.
fn render_runs_view(frame: &mut Frame, area: Rect, state: &WorkerUiState) {
    // Combine active and completed runs
    let mut all_runs: Vec<_> = state.active_runs.iter().collect();
    all_runs.extend(state.completed_runs.iter());

    if all_runs.is_empty() {
        let empty = Paragraph::new("No runs yet. Waiting for tasks...")
            .style(Style::default().fg(Color::DarkGray))
            .block(Block::default().borders(Borders::ALL).title(" Runs "));
        frame.render_widget(empty, area);
        return;
    }

    let columns = vec![
        TableColumn::new("Status", 10),
        TableColumn::new("Run ID", 10),
        TableColumn::new("Task ID", 10),
        TableColumn::new("Agent", 15),
        TableColumn::new("Started", 10),
        TableColumn::flex("Duration", 10),
    ];

    let rows: Vec<TableRow> = all_runs
        .iter()
        .map(|run| {
            let (status_str, status_color) = match run.status {
                RunStatus::Running => ("Running", Color::Yellow),
                RunStatus::Completed => ("Done", Color::Green),
                RunStatus::Failed => ("Failed", Color::Red),
            };

            let duration = if let Some(completed) = run.completed_at {
                let dur = completed.signed_duration_since(run.started_at);
                format!("{}s", dur.num_seconds())
            } else {
                let dur = chrono::Utc::now().signed_duration_since(run.started_at);
                format!("{}s...", dur.num_seconds())
            };

            TableRow::new(vec![
                TableCell::new(status_str).color(status_color),
                TableCell::new(run.run_id.chars().take(8).collect::<String>()),
                TableCell::new(run.task_id.chars().take(8).collect::<String>()),
                TableCell::new(run.agent.clone()),
                TableCell::muted(run.started_at.format("%H:%M:%S").to_string()),
                TableCell::new(duration),
            ])
        })
        .collect();

    DataTable::new(&columns, &rows)
        .title(format!(
            " Runs ({} active, {} completed) ",
            state.active_runs.len(),
            state.completed_runs.len()
        ))
        .selected(state.selected_run_index)
        .render(frame, area);
}

/// Render the run detail view as a chat interface using the shared RunDetailView.
fn render_run_detail_view(frame: &mut Frame, area: Rect, state: &WorkerUiState) {
    let run = match state.get_viewing_run() {
        Some(run) => run,
        None => {
            let empty = Paragraph::new("No run selected")
                .style(Style::default().fg(Color::DarkGray))
                .block(Block::default().borders(Borders::ALL).title(" Chat "));
            frame.render_widget(empty, area);
            return;
        }
    };

    // Convert worker's RunInfo to shared RunDetailInfo
    let run_detail_info = convert_run_info_to_detail(run);

    // Convert DetailPane to shared DetailPane
    let focused_pane = match state.detail_pane {
        DetailPane::Output => {
            if state.input_focused {
                SharedDetailPane::Input
            } else {
                SharedDetailPane::Chat
            }
        }
        DetailPane::Events => SharedDetailPane::Events,
    };

    // Render using the shared component
    RunDetailView::new(&run_detail_info)
        .focused_pane(focused_pane)
        .chat_scroll(state.chat_scroll)
        .events_scroll(state.events_scroll)
        .input(&state.chat_input, state.chat_input_cursor)
        .render(frame, area);
}

/// Convert worker's RunInfo to shared RunDetailInfo.
fn convert_run_info_to_detail(run: &RunInfo) -> RunDetailInfo {
    // Convert messages
    let messages: Vec<RunMessage> = run
        .messages
        .iter()
        .map(|msg| RunMessage {
            role: match msg.role {
                ChatRole::User => MessageRole::User,
                ChatRole::Assistant => MessageRole::Assistant,
            },
            content: msg.content.clone(),
            timestamp: msg.timestamp,
        })
        .collect();

    // Convert events
    let events: Vec<RunEvent> = run
        .events
        .iter()
        .map(|event| RunEvent {
            event_type: event.event_type.clone(),
            timestamp: event.timestamp,
            details: event.details.clone(),
        })
        .collect();

    // Convert status
    let status = match run.status {
        RunStatus::Running => RunDetailStatus::Running,
        RunStatus::Completed => RunDetailStatus::Completed,
        RunStatus::Failed => RunDetailStatus::Failed,
    };

    RunDetailInfo {
        run_id: run.run_id.clone(),
        task_id: run.task_id.clone(),
        agent: run.agent.clone(),
        status,
        started_at: run.started_at,
        completed_at: run.completed_at,
        messages,
        events,
        current_output: run.current_output.clone(),
        queued_input: run.queued_input.clone(),
    }
}

/// Render the logs view.
fn render_logs_view(frame: &mut Frame, area: Rect, state: &WorkerUiState) {
    let entries: Vec<_> = state.log_messages.iter().cloned().collect();

    LogsWidget::new(&entries)
        .scroll(state.log_scroll_offset)
        .render(frame, area);
}

/// Render the config view.
fn render_config_view(frame: &mut Frame, area: Rect, state: &WorkerUiState) {
    let (provider, model) = state.config.parse_model();

    let mut lines = vec![
        Line::from(vec![
            Span::styled("Agent Name:        ", Style::default().fg(Color::DarkGray)),
            Span::raw(&state.config.agent_name),
        ]),
        Line::from(vec![
            Span::styled("Model Provider:    ", Style::default().fg(Color::DarkGray)),
            Span::raw(provider),
        ]),
        Line::from(vec![
            Span::styled("Model Name:        ", Style::default().fg(Color::DarkGray)),
            Span::raw(model),
        ]),
        Line::from(vec![
            Span::styled("Working Dir:       ", Style::default().fg(Color::DarkGray)),
            Span::raw(&state.config.working_dir),
        ]),
        Line::from(vec![
            Span::styled("Control Plane:     ", Style::default().fg(Color::DarkGray)),
            Span::raw(&state.config.endpoint),
        ]),
        Line::from(vec![
            Span::styled("Max Concurrent:    ", Style::default().fg(Color::DarkGray)),
            Span::raw(state.config.max_concurrent_runs.to_string()),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("CA Certificate:    ", Style::default().fg(Color::DarkGray)),
            Span::raw(&state.config.ca_cert_path),
        ]),
        Line::from(vec![
            Span::styled("Client Cert:       ", Style::default().fg(Color::DarkGray)),
            Span::raw(&state.config.client_cert_path),
        ]),
        Line::from(vec![
            Span::styled("Client Key:        ", Style::default().fg(Color::DarkGray)),
            Span::raw(&state.config.client_key_path),
        ]),
    ];

    // Tool permissions
    lines.push(Line::from(""));
    if let Some(ref allowed) = state.config.allowed_tools {
        lines.push(Line::from(vec![
            Span::styled("Allowed Tools:     ", Style::default().fg(Color::DarkGray)),
            Span::styled(allowed.join(", "), Style::default().fg(Color::Green)),
        ]));
    } else {
        lines.push(Line::from(vec![
            Span::styled("Allowed Tools:     ", Style::default().fg(Color::DarkGray)),
            Span::styled("(all)", Style::default().fg(Color::DarkGray)),
        ]));
    }

    if let Some(ref denied) = state.config.denied_tools {
        lines.push(Line::from(vec![
            Span::styled("Denied Tools:      ", Style::default().fg(Color::DarkGray)),
            Span::styled(denied.join(", "), Style::default().fg(Color::Red)),
        ]));
    } else {
        lines.push(Line::from(vec![
            Span::styled("Denied Tools:      ", Style::default().fg(Color::DarkGray)),
            Span::styled("(none)", Style::default().fg(Color::DarkGray)),
        ]));
    }

    let config = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Configuration "),
    );

    frame.render_widget(config, area);
}

/// Render quit confirmation dialog.
fn render_quit_confirm(frame: &mut Frame) {
    ConfirmDialog::new("Confirm", "Quit worker?").render(frame);
}

/// Render new run dialog.
fn render_new_run_dialog(frame: &mut Frame, state: &WorkerUiState) {
    InputDialog::new(
        "New Task",
        "Enter prompt for new task:",
        &state.new_run_prompt,
    )
    .cursor(state.new_run_cursor)
    .render(frame);
}
