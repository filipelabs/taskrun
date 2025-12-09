//! UI rendering for the worker TUI.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, List, ListItem, Paragraph, Row, Table, Tabs};
use ratatui::Frame;

use super::state::{
    ChatRole, ConnectionState, DetailPane, LogLevel, RunStatus, WorkerUiState, WorkerView,
};

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

    // Render quit confirmation dialog on top if shown
    if state.show_quit_confirm {
        render_quit_confirm(frame);
    }

    // Render new run dialog on top if shown
    if state.show_new_run_dialog {
        render_new_run_dialog(frame, state);
    }
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
        WorkerView::RunDetail => render_run_detail_view(frame, area, state),
        WorkerView::Logs => render_logs_view(frame, area, state),
        WorkerView::Config => render_config_view(frame, area, state),
    }
}

/// Render the status view.
fn render_status_view(frame: &mut Frame, area: Rect, state: &WorkerUiState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(11), // Worker info
            Constraint::Min(0),     // Stats
        ])
        .split(area);

    // Worker info
    let connection_status = match &state.connection_state {
        ConnectionState::Connecting => {
            Span::styled("Connecting...", Style::default().fg(Color::Yellow))
        }
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
        Line::from(vec![
            Span::raw("Working Dir: "),
            Span::styled(&state.config.working_dir, Style::default().fg(Color::Cyan)),
        ]),
        Line::from(vec![Span::raw("Connection:  "), connection_status]),
        Line::from(vec![
            Span::raw("Uptime:      "),
            Span::styled(uptime_str, Style::default().fg(Color::Cyan)),
        ]),
        Line::from(vec![
            Span::raw("Active Runs: "),
            Span::styled(
                format!(
                    "{}/{}",
                    state.active_runs.len(),
                    state.config.max_concurrent_runs
                ),
                Style::default().fg(if state.active_runs.is_empty() {
                    Color::Gray
                } else {
                    Color::Green
                }),
            ),
        ]),
    ];

    let info = Paragraph::new(info_lines).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Worker Info "),
    );
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
                        (state.stats.successful_runs as f64 / state.stats.total_runs as f64)
                            * 100.0
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
    .block(Block::default().borders(Borders::ALL).title(format!(
        " Runs ({} active, {} completed) ",
        state.active_runs.len(),
        state.completed_runs.len()
    )));

    frame.render_widget(table, area);
}

/// Render the run detail view as a chat interface.
fn render_run_detail_view(frame: &mut Frame, area: Rect, state: &WorkerUiState) {
    let run = match state.get_viewing_run() {
        Some(run) => run,
        None => {
            let empty = Paragraph::new("No run selected")
                .style(Style::default().fg(Color::Gray))
                .block(Block::default().borders(Borders::ALL).title(" Chat "));
            frame.render_widget(empty, area);
            return;
        }
    };

    // Layout: header + chat/events split + input box
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Status header
            Constraint::Min(0),    // Chat + events
            Constraint::Length(3), // Input box
        ])
        .split(area);

    // Render status header
    render_chat_header(frame, chunks[0], run);

    // Split chat and events
    let content_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(70), // Chat (wider)
            Constraint::Percentage(30), // Events
        ])
        .split(chunks[1]);

    // Render chat messages
    render_chat_messages(frame, content_chunks[0], run, state);

    // Render events pane
    render_events_pane(frame, content_chunks[1], run, state);

    // Render input box
    render_chat_input(frame, chunks[2], run, state);
}

/// Render the chat status header.
fn render_chat_header(frame: &mut Frame, area: Rect, run: &super::state::RunInfo) {
    let status_style = match run.status {
        RunStatus::Running => Style::default().fg(Color::Yellow),
        RunStatus::Completed => Style::default().fg(Color::Green),
        RunStatus::Failed => Style::default().fg(Color::Red),
    };
    let status_str = match run.status {
        RunStatus::Running => "● Running",
        RunStatus::Completed => "✓ Completed",
        RunStatus::Failed => "✗ Failed",
    };

    let duration = if let Some(completed) = run.completed_at {
        let dur = completed.signed_duration_since(run.started_at);
        format!("{}s", dur.num_seconds())
    } else {
        let dur = chrono::Utc::now().signed_duration_since(run.started_at);
        format!("{}s", dur.num_seconds())
    };

    let header = Paragraph::new(Line::from(vec![
        Span::styled(status_str, status_style),
        Span::raw(" | "),
        Span::raw("Agent: "),
        Span::styled(&run.agent, Style::default().fg(Color::Cyan)),
        Span::raw(" | "),
        Span::styled(duration, Style::default().fg(Color::DarkGray)),
        Span::raw(" | "),
        Span::styled(
            format!("{} messages", run.messages.len()),
            Style::default().fg(Color::DarkGray),
        ),
    ]))
    .block(Block::default().borders(Borders::ALL));

    frame.render_widget(header, area);
}

/// Wrap text to fit within a given width (unicode-safe).
fn wrap_text(text: &str, width: usize, indent: &str) -> Vec<String> {
    let mut lines = Vec::new();
    let effective_width = width.saturating_sub(indent.chars().count());

    if effective_width == 0 {
        return vec![format!("{}{}", indent, text)];
    }

    for line in text.lines() {
        if line.is_empty() {
            lines.push(indent.to_string());
            continue;
        }

        let chars: Vec<char> = line.chars().collect();
        let mut start = 0;

        while start < chars.len() {
            let remaining_chars = chars.len() - start;

            if remaining_chars <= effective_width {
                let remaining: String = chars[start..].iter().collect();
                lines.push(format!("{}{}", indent, remaining));
                break;
            }

            // Find a good break point (prefer space within effective_width)
            let end = start + effective_width;
            let search_range: String = chars[start..end].iter().collect();

            let break_offset = search_range.rfind(' ').unwrap_or(effective_width);
            let actual_end = start + break_offset;

            let chunk: String = chars[start..actual_end].iter().collect();
            lines.push(format!("{}{}", indent, chunk.trim_end()));

            // Skip past the space
            start = actual_end;
            while start < chars.len() && chars[start] == ' ' {
                start += 1;
            }
        }
    }

    if lines.is_empty() {
        lines.push(indent.to_string());
    }

    lines
}

/// Render chat messages.
fn render_chat_messages(
    frame: &mut Frame,
    area: Rect,
    run: &super::state::RunInfo,
    state: &WorkerUiState,
) {
    let is_focused = state.detail_pane == DetailPane::Output && !state.input_focused;
    let border_style = if is_focused {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let visible_height = area.height.saturating_sub(2) as usize;
    // Available width for text (minus borders)
    let text_width = area.width.saturating_sub(2) as usize;

    // Build all message lines
    let mut all_lines: Vec<Line> = Vec::new();

    for msg in &run.messages {
        let (prefix, style) = match msg.role {
            ChatRole::User => ("You: ", Style::default().fg(Color::Green)),
            ChatRole::Assistant => ("AI: ", Style::default().fg(Color::Cyan)),
        };

        // Add message header
        all_lines.push(Line::from(vec![
            Span::styled(prefix, style.add_modifier(Modifier::BOLD)),
            Span::styled(
                msg.timestamp.format("%H:%M:%S").to_string(),
                Style::default().fg(Color::DarkGray),
            ),
        ]));

        // Add message content with word wrapping
        for wrapped_line in wrap_text(&msg.content, text_width, "  ") {
            all_lines.push(Line::from(Span::raw(wrapped_line)));
        }

        // Add blank line between messages
        all_lines.push(Line::from(""));
    }

    // If there's streaming output, show it
    if !run.current_output.is_empty() {
        all_lines.push(Line::from(vec![
            Span::styled(
                "AI: ",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("(streaming...)", Style::default().fg(Color::DarkGray)),
        ]));
        for wrapped_line in wrap_text(&run.current_output, text_width, "  ") {
            all_lines.push(Line::from(Span::raw(wrapped_line)));
        }
    }

    let total_lines = all_lines.len();

    // Auto-scroll to bottom (usize::MAX), or use manual scroll position
    let max_scroll = total_lines.saturating_sub(visible_height);
    let scroll_offset = if state.chat_scroll == usize::MAX {
        max_scroll // Auto-scroll to bottom
    } else {
        state.chat_scroll.min(max_scroll)
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

/// Render the chat input box.
fn render_chat_input(
    frame: &mut Frame,
    area: Rect,
    run: &super::state::RunInfo,
    state: &WorkerUiState,
) {
    let border_style = if state.input_focused {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    // Show queued message or input prompt
    let (title, content) = if let Some(ref queued) = run.queued_input {
        (" Queued (will send when run completes) ", queued.clone())
    } else if run.status == RunStatus::Running {
        (
            " Type message (queued until run completes) ",
            state.chat_input.clone(),
        )
    } else {
        (" Type message (Enter to send) ", state.chat_input.clone())
    };

    // Add cursor (unicode-safe: convert char index to byte index)
    let display_text = if state.input_focused && run.queued_input.is_none() {
        let chars: Vec<char> = state.chat_input.chars().collect();
        let cursor_pos = state.chat_input_cursor.min(chars.len());
        let before: String = chars[..cursor_pos].iter().collect();
        let after: String = chars[cursor_pos..].iter().collect();
        format!("{}│{}", before, after)
    } else {
        content.clone()
    };

    let input = Paragraph::new(display_text)
        .style(if run.queued_input.is_some() {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default().fg(Color::White)
        })
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(border_style)
                .title(title),
        );

    frame.render_widget(input, area);
}

/// Render the events pane in run detail view.
fn render_events_pane(
    frame: &mut Frame,
    area: Rect,
    run: &super::state::RunInfo,
    state: &WorkerUiState,
) {
    let is_focused = state.detail_pane == DetailPane::Events;
    let border_style = if is_focused {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let visible_height = area.height.saturating_sub(2) as usize;
    let total_events = run.events.len();

    // Clamp scroll offset
    let max_scroll = total_events.saturating_sub(visible_height);
    let scroll_offset = state.events_scroll.min(max_scroll);

    let items: Vec<ListItem> = run
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
                Span::styled(
                    format!("{} ", timestamp),
                    Style::default().fg(Color::DarkGray),
                ),
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
                Span::styled(
                    format!("{} ", timestamp),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::styled(format!("[{:<5}] ", level_str), level_style),
                Span::raw(&entry.message),
            ]))
        })
        .collect();

    let list = List::new(items).block(Block::default().borders(Borders::ALL).title(format!(
        " Logs ({}/{}) ",
        state.log_scroll_offset + 1,
        state.log_messages.len()
    )));

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
            Span::styled("Working Dir:       ", Style::default().fg(Color::Gray)),
            Span::raw(&state.config.working_dir),
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

    let config = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Configuration "),
    );

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
        WorkerView::Runs => {
            "[n] New  [Enter] Join  [j/k] Navigate  [1-4] Views  [Tab] Next  [q] Quit"
        }
        WorkerView::RunDetail => "[Esc] Back  [Tab] Switch pane  [j/k] Scroll  [g/G] Top/Bottom",
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

/// Render quit confirmation dialog.
fn render_quit_confirm(frame: &mut Frame) {
    use ratatui::widgets::Clear;

    let area = frame.area();

    // Center the popup
    let popup_width = 40;
    let popup_height = 5;
    let popup_x = (area.width.saturating_sub(popup_width)) / 2;
    let popup_y = (area.height.saturating_sub(popup_height)) / 2;

    let popup_area = Rect::new(popup_x, popup_y, popup_width, popup_height);

    // Clear the area behind the popup
    frame.render_widget(Clear, popup_area);

    // Render the popup
    let text = vec![
        Line::from(""),
        Line::from(Span::styled(
            "  Quit worker?",
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            "  [Y]es  [N]o",
            Style::default().fg(Color::DarkGray),
        )),
    ];

    let popup = Paragraph::new(text).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Yellow))
            .title(" Confirm "),
    );

    frame.render_widget(popup, popup_area);
}

/// Render new run dialog.
fn render_new_run_dialog(frame: &mut Frame, state: &WorkerUiState) {
    use ratatui::widgets::Clear;

    let area = frame.area();

    // Center the popup (wider for input)
    let popup_width = 60.min(area.width.saturating_sub(4));
    let popup_height = 7;
    let popup_x = (area.width.saturating_sub(popup_width)) / 2;
    let popup_y = (area.height.saturating_sub(popup_height)) / 2;

    let popup_area = Rect::new(popup_x, popup_y, popup_width, popup_height);

    // Clear the area behind the popup
    frame.render_widget(Clear, popup_area);

    // Build input line with cursor (unicode-safe)
    let input = &state.new_run_prompt;
    let cursor_pos = state.new_run_cursor;
    let chars: Vec<char> = input.chars().collect();
    let input_display = if cursor_pos < chars.len() {
        let before: String = chars[..cursor_pos].iter().collect();
        let after: String = chars[cursor_pos..].iter().collect();
        format!("  {}█{}", before, after)
    } else {
        format!("  {}█", input)
    };

    let text = vec![
        Line::from(""),
        Line::from(Span::styled(
            "  Enter prompt for new task:",
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(
            input_display,
            Style::default().fg(Color::White),
        )),
        Line::from(Span::styled(
            "  [Enter] Submit  [Esc] Cancel",
            Style::default().fg(Color::DarkGray),
        )),
    ];

    let popup = Paragraph::new(text).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan))
            .title(" New Task "),
    );

    frame.render_widget(popup, popup_area);
}
