//! Tasks view.

use ratatui::Frame;
use ratatui::layout::{Constraint, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, Cell, Row, Table, TableState};

use taskrun_core::TaskStatus;

use crate::state::ServerUiState;

pub fn render_tasks_view(f: &mut Frame, state: &ServerUiState, area: Rect) {
    let tasks = state.task_display_list();

    let header = Row::new(vec![
        Cell::from("Task ID"),
        Cell::from("Agent"),
        Cell::from("Status"),
        Cell::from("Created"),
        Cell::from("Runs"),
        Cell::from("Latest Run"),
    ])
    .style(Style::default().add_modifier(Modifier::BOLD))
    .height(1);

    let rows: Vec<Row> = tasks
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let status_style = match t.status {
                TaskStatus::Pending => Style::default().fg(Color::Yellow),
                TaskStatus::Running => Style::default().fg(Color::Cyan),
                TaskStatus::Completed => Style::default().fg(Color::Green),
                TaskStatus::Failed => Style::default().fg(Color::Red),
                TaskStatus::Cancelled => Style::default().fg(Color::DarkGray),
            };

            let created_ago = chrono::Utc::now()
                .signed_duration_since(t.created_at)
                .num_seconds();
            let created_str = if created_ago < 60 {
                format!("{}s ago", created_ago)
            } else if created_ago < 3600 {
                format!("{}m ago", created_ago / 60)
            } else {
                format!("{}h ago", created_ago / 3600)
            };

            let latest_run_str = match &t.latest_run_status {
                Some(status) => format!("{:?}", status),
                None => "-".to_string(),
            };

            let row_style = if i == state.selected_task_index {
                Style::default().bg(Color::DarkGray)
            } else {
                Style::default()
            };

            Row::new(vec![
                Cell::from(t.task_id.to_string()[..8].to_string()),
                Cell::from(t.agent_name.clone()),
                Cell::from(format!("{:?}", t.status)).style(status_style),
                Cell::from(created_str),
                Cell::from(format!("{}", t.run_count)),
                Cell::from(latest_run_str),
            ])
            .style(row_style)
            .height(1)
        })
        .collect();

    let widths = [
        Constraint::Length(10),  // Task ID
        Constraint::Length(20),  // Agent
        Constraint::Length(12),  // Status
        Constraint::Length(12),  // Created
        Constraint::Length(6),   // Runs
        Constraint::Min(12),     // Latest Run
    ];

    let table = Table::new(rows, widths)
        .header(header)
        .block(
            Block::default()
                .title(format!(" Tasks ({}) ", state.task_list.len()))
                .borders(Borders::ALL),
        )
        .row_highlight_style(Style::default().add_modifier(Modifier::REVERSED));

    let mut table_state = TableState::default();
    if !tasks.is_empty() {
        table_state.select(Some(state.selected_task_index));
    }

    f.render_stateful_widget(table, area, &mut table_state);
}
