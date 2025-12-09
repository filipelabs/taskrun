//! Tasks view.

use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::Frame;

use taskrun_core::TaskStatus;
use taskrun_tui_components::{DataTable, TableCell, TableColumn, TableRow};

use crate::state::ServerUiState;

pub fn render_tasks_view(f: &mut Frame, state: &ServerUiState, area: Rect) {
    let tasks = state.task_display_list();

    let columns = vec![
        TableColumn::new("Task ID", 10),
        TableColumn::new("Agent", 20),
        TableColumn::new("Status", 12),
        TableColumn::new("Created", 12),
        TableColumn::new("Runs", 6),
        TableColumn::flex("Latest Run", 12),
    ];

    let rows: Vec<TableRow> = tasks
        .iter()
        .map(|t| {
            let status_color = match t.status {
                TaskStatus::Pending => Color::Yellow,
                TaskStatus::Running => Color::Cyan,
                TaskStatus::Completed => Color::Green,
                TaskStatus::Failed => Color::Red,
                TaskStatus::Cancelled => Color::DarkGray,
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

            TableRow::new(vec![
                TableCell::new(t.task_id.to_string()[..8].to_string()),
                TableCell::new(t.agent_name.clone()),
                TableCell::new(format!("{:?}", t.status)).color(status_color),
                TableCell::muted(created_str),
                TableCell::new(format!("{}", t.run_count)),
                TableCell::new(latest_run_str),
            ])
        })
        .collect();

    DataTable::new(&columns, &rows)
        .title(format!(" Tasks ({}) ", state.task_list.len()))
        .selected(state.selected_task_index)
        .render(f, area);
}
