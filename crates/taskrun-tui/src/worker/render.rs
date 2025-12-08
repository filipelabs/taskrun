//! UI rendering for the worker TUI.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, List, ListItem, Paragraph, Row, Table, Tabs};
use ratatui::Frame;

use super::state::{ConnectionState, LogLevel, RunStatus, WorkerUiState, WorkerView};

/// Main render function for the worker TUI.
pub fn render(frame: &mut Frame, state: &WorkerUiState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header with tabs
            Constraint::Min(0),    // Main content
            Constraint::Length(3), // Footer with status/help
        ])
        .split(frame.area());

    render_header(frame, chunks[0], state);
    render_main_content(frame, chunks[1], state);
    render_footer(frame, chunks[2], state);
}

/// Render the header with tabs.
fn render_header(frame: &mut Frame, area: Rect, state: &WorkerUiState) {
    let titles: Vec<&str> = WorkerView::all().iter().map(|v| v.name()).collect();

    let tabs = Tabs::new(titles)
        .select(
            WorkerView::all()
                .iter()
                .position(|v| *v == state.current_view)
                .unwrap_or(0),
        )
        .style(Style::default().fg(Color::White))
        .highlight_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )
        .divider(" | ")
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!(" Worker TUI - {} ", state.worker_id)),
        );

    frame.render_widget(tabs, area);
}

/// Render the main content area based on current view.
fn render_main_content(frame: &mut Frame, area: Rect, state: &WorkerUiState) {
    match state.current_view {
        WorkerView::Status => render_status_view(frame, area, state),
        WorkerView::Runs => render_runs_view(frame, area, state),
        WorkerView::Logs => render_logs_view(frame, area, state),
        WorkerView::Config => render_config_view(frame, area, state),
    }
}

/// Render the status view.
fn render_status_view(frame: &mut Frame, area: Rect, state: &WorkerUiState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(10), // Worker info
            Constraint::Min(0),     // Stats
        ])
        .split(area);

    // Worker info
    let connection_status = match &state.connection_state {
        ConnectionState::Connecting => Span::styled("Connecting...", Style::default().fg(Color::Yellow)),
        ConnectionState::Connected => Span::styled("Connected", Style::default().fg(Color::Green)),
        ConnectionState::Disconnected { retry_in } => Span::styled(
            format!("Disconnected (retry in {}s)", retry_in.as_secs()),
            Style::default().fg(Color::Red),
        ),
    };

    let (provider, model) = state.config.parse_model();
    let uptime = state.uptime();
    let uptime_str = format!(
        "{}h {:02}m {:02}s",
        uptime.as_secs() / 3600,
        (uptime.as_secs() % 3600) / 60,
        uptime.as_secs() % 60
    );

    let info_lines = vec![
        Line::from(vec![
            Span::raw("Worker ID:   "),
            Span::styled(&state.worker_id, Style::default().fg(Color::Cyan)),
        ]),
        Line::from(vec![
            Span::raw("Agent:       "),
            Span::styled(&state.config.agent_name, Style::default().fg(Color::Cyan)),
        ]),
        Line::from(vec![
            Span::raw("Model:       "),
            Span::styled(
                format!("{}/{}", provider, model),
                Style::default().fg(Color::Cyan),
            ),
        ]),
        Line::from(vec![Span::raw("Connection:  "), connection_status]),
        Line::from(vec![
            Span::raw("Uptime:      "),
            Span::styled(uptime_str, Style::default().fg(Color::Cyan)),
        ]),
        Line::from(vec![
            Span::raw("Active Runs: "),
            Span::styled(
                format!("{}/{}", state.active_runs.len(), state.config.max_concurrent_runs),
                Style::default().fg(if state.active_runs.is_empty() {
                    Color::Gray
                } else {
                    Color::Green
                }),
            ),
        ]),
    ];

    let info = Paragraph::new(info_lines)
        .block(Block::default().borders(Borders::ALL).title(" Worker Info "));
    frame.render_widget(info, chunks[0]);

    // Stats
    let stats_lines = vec![
        Line::from(vec![
            Span::raw("Total Runs:      "),
            Span::styled(
                state.stats.total_runs.to_string(),
                Style::default().fg(Color::Cyan),
            ),
        ]),
        Line::from(vec![
            Span::raw("Successful:      "),
            Span::styled(
                state.stats.successful_runs.to_string(),
                Style::default().fg(Color::Green),
            ),
        ]),
        Line::from(vec![
            Span::raw("Failed:          "),
            Span::styled(
                state.stats.failed_runs.to_string(),
                Style::default().fg(if state.stats.failed_runs > 0 {
                    Color::Red
                } else {
                    Color::Gray
                }),
            ),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::raw("Success Rate:    "),
            Span::styled(
                if state.stats.total_runs > 0 {
                    format!(
                        "{:.1}%",
                        (state.stats.successful_runs as f64 / state.stats.total_runs as f64) * 100.0
                    )
                } else {
                    "N/A".to_string()
                },
                Style::default().fg(Color::Cyan),
            ),
        ]),
    ];

    let stats = Paragraph::new(stats_lines)
        .block(Block::default().borders(Borders::ALL).title(" Statistics "));
    frame.render_widget(stats, chunks[1]);
}

/// Render the runs view.
fn render_runs_view(frame: &mut Frame, area: Rect, state: &WorkerUiState) {
    // Combine active and completed runs
    let mut all_runs: Vec<_> = state.active_runs.iter().collect();
    all_runs.extend(state.completed_runs.iter());

    if all_runs.is_empty() {
        let empty = Paragraph::new("No runs yet. Waiting for tasks...")
            .style(Style::default().fg(Color::Gray))
            .block(Block::default().borders(Borders::ALL).title(" Runs "));
        frame.render_widget(empty, area);
        return;
    }

    let header = Row::new(vec![
        Cell::from("Status"),
        Cell::from("Run ID"),
        Cell::from("Task ID"),
        Cell::from("Agent"),
        Cell::from("Started"),
        Cell::from("Duration"),
    ])
    .style(Style::default().add_modifier(Modifier::BOLD))
    .height(1);

    let rows: Vec<Row> = all_runs
        .iter()
        .enumerate()
        .map(|(i, run)| {
            let status_style = match run.status {
                RunStatus::Running => Style::default().fg(Color::Yellow),
                RunStatus::Completed => Style::default().fg(Color::Green),
                RunStatus::Failed => Style::default().fg(Color::Red),
            };

            let status_str = match run.status {
                RunStatus::Running => "Running",
                RunStatus::Completed => "Done",
                RunStatus::Failed => "Failed",
            };

            let duration = if let Some(completed) = run.completed_at {
                let dur = completed.signed_duration_since(run.started_at);
                format!("{}s", dur.num_seconds())
            } else {
                let dur = chrono::Utc::now().signed_duration_since(run.started_at);
                format!("{}s...", dur.num_seconds())
            };

            let style = if i == state.selected_run_index {
                Style::default().bg(Color::DarkGray)
            } else {
                Style::default()
            };

            Row::new(vec![
                Cell::from(status_str).style(status_style),
                Cell::from(run.run_id.chars().take(8).collect::<String>()),
                Cell::from(run.task_id.chars().take(8).collect::<String>()),
                Cell::from(run.agent.clone()),
                Cell::from(run.started_at.format("%H:%M:%S").to_string()),
                Cell::from(duration),
            ])
            .style(style)
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(10),
            Constraint::Length(10),
            Constraint::Length(10),
            Constraint::Length(15),
            Constraint::Length(10),
            Constraint::Min(10),
        ],
    )
    .header(header)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(format!(" Runs ({} active, {} completed) ", state.active_runs.len(), state.completed_runs.len())),
    );

    frame.render_widget(table, area);
}

/// Render the logs view.
fn render_logs_view(frame: &mut Frame, area: Rect, state: &WorkerUiState) {
    if state.log_messages.is_empty() {
        let empty = Paragraph::new("No log messages yet.")
            .style(Style::default().fg(Color::Gray))
            .block(Block::default().borders(Borders::ALL).title(" Logs "));
        frame.render_widget(empty, area);
        return;
    }

    let visible_height = area.height.saturating_sub(2) as usize;
    let items: Vec<ListItem> = state
        .log_messages
        .iter()
        .skip(state.log_scroll_offset)
        .take(visible_height)
        .map(|entry| {
            let level_style = match entry.level {
                LogLevel::Debug => Style::default().fg(Color::DarkGray),
                LogLevel::Info => Style::default().fg(Color::Blue),
                LogLevel::Warn => Style::default().fg(Color::Yellow),
                LogLevel::Error => Style::default().fg(Color::Red),
            };

            let level_str = entry.level.as_str();
            let timestamp = entry.timestamp.format("%H:%M:%S");

            ListItem::new(Line::from(vec![
                Span::styled(format!("{} ", timestamp), Style::default().fg(Color::DarkGray)),
                Span::styled(format!("[{:<5}] ", level_str), level_style),
                Span::raw(&entry.message),
            ]))
        })
        .collect();

    let list = List::new(items).block(
        Block::default().borders(Borders::ALL).title(format!(
            " Logs ({}/{}) ",
            state.log_scroll_offset + 1,
            state.log_messages.len()
        )),
    );

    frame.render_widget(list, area);
}

/// Render the config view.
fn render_config_view(frame: &mut Frame, area: Rect, state: &WorkerUiState) {
    let (provider, model) = state.config.parse_model();

    let mut lines = vec![
        Line::from(vec![
            Span::styled("Agent Name:        ", Style::default().fg(Color::Gray)),
            Span::raw(&state.config.agent_name),
        ]),
        Line::from(vec![
            Span::styled("Model Provider:    ", Style::default().fg(Color::Gray)),
            Span::raw(provider),
        ]),
        Line::from(vec![
            Span::styled("Model Name:        ", Style::default().fg(Color::Gray)),
            Span::raw(model),
        ]),
        Line::from(vec![
            Span::styled("Control Plane:     ", Style::default().fg(Color::Gray)),
            Span::raw(&state.config.endpoint),
        ]),
        Line::from(vec![
            Span::styled("Max Concurrent:    ", Style::default().fg(Color::Gray)),
            Span::raw(state.config.max_concurrent_runs.to_string()),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("CA Certificate:    ", Style::default().fg(Color::Gray)),
            Span::raw(&state.config.ca_cert_path),
        ]),
        Line::from(vec![
            Span::styled("Client Cert:       ", Style::default().fg(Color::Gray)),
            Span::raw(&state.config.client_cert_path),
        ]),
        Line::from(vec![
            Span::styled("Client Key:        ", Style::default().fg(Color::Gray)),
            Span::raw(&state.config.client_key_path),
        ]),
    ];

    // Tool permissions
    lines.push(Line::from(""));
    if let Some(ref allowed) = state.config.allowed_tools {
        lines.push(Line::from(vec![
            Span::styled("Allowed Tools:     ", Style::default().fg(Color::Gray)),
            Span::styled(allowed.join(", "), Style::default().fg(Color::Green)),
        ]));
    } else {
        lines.push(Line::from(vec![
            Span::styled("Allowed Tools:     ", Style::default().fg(Color::Gray)),
            Span::styled("(all)", Style::default().fg(Color::DarkGray)),
        ]));
    }

    if let Some(ref denied) = state.config.denied_tools {
        lines.push(Line::from(vec![
            Span::styled("Denied Tools:      ", Style::default().fg(Color::Gray)),
            Span::styled(denied.join(", "), Style::default().fg(Color::Red)),
        ]));
    } else {
        lines.push(Line::from(vec![
            Span::styled("Denied Tools:      ", Style::default().fg(Color::Gray)),
            Span::styled("(none)", Style::default().fg(Color::DarkGray)),
        ]));
    }

    let config = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title(" Configuration "));

    frame.render_widget(config, area);
}

/// Render the footer with status and keybindings.
fn render_footer(frame: &mut Frame, area: Rect, state: &WorkerUiState) {
    let status = state
        .status_message
        .clone()
        .unwrap_or_else(|| "Ready".to_string());

    let help = match state.current_view {
        WorkerView::Status => "[1-4] Views  [Tab] Next  [q] Quit",
        WorkerView::Runs => "[j/k] Navigate  [1-4] Views  [Tab] Next  [q] Quit",
        WorkerView::Logs => "[j/k] Scroll  [1-4] Views  [Tab] Next  [q] Quit",
        WorkerView::Config => "[1-4] Views  [Tab] Next  [q] Quit",
    };

    let footer = Paragraph::new(Line::from(vec![
        Span::raw(status),
        Span::raw(" | "),
        Span::styled(help, Style::default().fg(Color::DarkGray)),
    ]))
    .block(Block::default().borders(Borders::ALL));

    frame.render_widget(footer, area);
}
