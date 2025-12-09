//! Logs view.

use ratatui::layout::Rect;
use ratatui::Frame;

use taskrun_tui_components::LogsWidget;

use crate::state::ServerUiState;

pub fn render_logs_view(f: &mut Frame, state: &ServerUiState, area: Rect) {
    // Convert VecDeque to slice for the widget
    let entries: Vec<_> = state.log_messages.iter().cloned().collect();

    LogsWidget::new(&entries)
        .scroll(state.log_scroll)
        .render(f, area);
}
