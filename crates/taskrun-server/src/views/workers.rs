//! Workers view.

use ratatui::layout::{Constraint, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, Cell, Row, Table, TableState};
use ratatui::Frame;

use taskrun_core::WorkerStatus;

use crate::state::ServerUiState;

pub fn render_workers_view(f: &mut Frame, state: &ServerUiState, area: Rect) {
    let workers = state.worker_list();

    let header = Row::new(vec![
        Cell::from("Worker ID"),
        Cell::from("Hostname"),
        Cell::from("Agents"),
        Cell::from("Status"),
        Cell::from("Runs"),
        Cell::from("Last Heartbeat"),
    ])
    .style(Style::default().add_modifier(Modifier::BOLD))
    .height(1);

    let rows: Vec<Row> = workers
        .iter()
        .enumerate()
        .map(|(i, w)| {
            let status_style = match w.status {
                WorkerStatus::Idle => Style::default().fg(Color::Green),
                WorkerStatus::Busy => Style::default().fg(Color::Yellow),
                WorkerStatus::Draining => Style::default().fg(Color::Magenta),
                WorkerStatus::Error => Style::default().fg(Color::Red),
            };

            let agents_str = if w.agents.len() <= 2 {
                w.agents.join(", ")
            } else {
                format!("{}, +{}", w.agents[0], w.agents.len() - 1)
            };

            let last_hb = chrono::Utc::now()
                .signed_duration_since(w.last_heartbeat)
                .num_seconds();
            let hb_str = if last_hb < 60 {
                format!("{}s ago", last_hb)
            } else {
                format!("{}m ago", last_hb / 60)
            };

            let row_style = if i == state.selected_worker_index {
                Style::default().bg(Color::DarkGray)
            } else {
                Style::default()
            };

            Row::new(vec![
                Cell::from(w.worker_id.to_string()[..8].to_string()),
                Cell::from(w.hostname.clone()),
                Cell::from(agents_str),
                Cell::from(format!("{:?}", w.status)).style(status_style),
                Cell::from(format!("{}/{}", w.active_runs, w.max_concurrent_runs)),
                Cell::from(hb_str),
            ])
            .style(row_style)
            .height(1)
        })
        .collect();

    let widths = [
        Constraint::Length(10), // Worker ID
        Constraint::Length(20), // Hostname
        Constraint::Min(15),    // Agents
        Constraint::Length(12), // Status
        Constraint::Length(8),  // Runs
        Constraint::Length(15), // Last Heartbeat
    ];

    let table = Table::new(rows, widths)
        .header(header)
        .block(
            Block::default()
                .title(format!(" Workers ({}) ", state.workers.len()))
                .borders(Borders::ALL),
        )
        .row_highlight_style(Style::default().add_modifier(Modifier::REVERSED));

    let mut table_state = TableState::default();
    if !workers.is_empty() {
        table_state.select(Some(state.selected_worker_index));
    }

    f.render_stateful_widget(table, area, &mut table_state);
}
