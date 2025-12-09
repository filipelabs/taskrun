//! Workers view.

use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::Frame;

use taskrun_core::WorkerStatus;
use taskrun_tui_components::{DataTable, TableCell, TableColumn, TableRow};

use crate::state::ServerUiState;

pub fn render_workers_view(f: &mut Frame, state: &ServerUiState, area: Rect) {
    let workers = state.worker_list();

    let columns = vec![
        TableColumn::new("Worker ID", 10),
        TableColumn::new("Hostname", 20),
        TableColumn::flex("Agents", 15),
        TableColumn::new("Status", 12),
        TableColumn::new("Runs", 8),
        TableColumn::new("Last Heartbeat", 15),
    ];

    let rows: Vec<TableRow> = workers
        .iter()
        .map(|w| {
            let status_color = match w.status {
                WorkerStatus::Idle => Color::Green,
                WorkerStatus::Busy => Color::Yellow,
                WorkerStatus::Draining => Color::Magenta,
                WorkerStatus::Error => Color::Red,
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

            TableRow::new(vec![
                TableCell::new(w.worker_id.to_string()[..8].to_string()),
                TableCell::new(w.hostname.clone()),
                TableCell::new(agents_str),
                TableCell::new(format!("{:?}", w.status)).color(status_color),
                TableCell::new(format!("{}/{}", w.active_runs, w.max_concurrent_runs)),
                TableCell::muted(hb_str),
            ])
        })
        .collect();

    DataTable::new(&columns, &rows)
        .title(format!(" Workers ({}) ", state.workers.len()))
        .selected(state.selected_worker_index)
        .render(f, area);
}
