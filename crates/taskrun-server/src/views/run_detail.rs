//! Run detail view using shared RunDetailView component.

use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use taskrun_core::{ChatRole, RunStatus, TaskStatus};
use taskrun_tui_components::{
    DetailPane, MessageRole, RunDetailInfo, RunDetailStatus, RunDetailView, RunEvent, RunMessage,
};

use crate::state::ServerUiState;

pub fn render_run_detail_view(f: &mut Frame, state: &ServerUiState, area: Rect) {
    let task = match state.get_viewing_task() {
        Some(t) => t,
        None => {
            let empty = Paragraph::new("No task selected")
                .style(Style::default().fg(Color::DarkGray))
                .block(Block::default().borders(Borders::ALL).title(" Run Detail "));
            f.render_widget(empty, area);
            return;
        }
    };

    let run_id = match &task.latest_run_id {
        Some(id) => id,
        None => {
            let empty = Paragraph::new("No runs yet")
                .style(Style::default().fg(Color::DarkGray))
                .block(Block::default().borders(Borders::ALL).title(" Run Detail "));
            f.render_widget(empty, area);
            return;
        }
    };

    // Convert server data to RunDetailInfo
    let run_detail_info = convert_to_run_detail(state, task, run_id);

    // Server always has input focused in run detail view
    let focused_pane = DetailPane::Input;

    // Render using the shared component
    RunDetailView::new(&run_detail_info)
        .focused_pane(focused_pane)
        .chat_scroll(state.run_scroll)
        .events_scroll(0) // Server doesn't track events scroll separately
        .input(&state.chat_input, state.chat_input_cursor)
        .render(f, area);
}

/// Convert server state data to RunDetailInfo for the shared widget.
fn convert_to_run_detail(
    state: &ServerUiState,
    task: &crate::state::TaskDisplayInfo,
    run_id: &taskrun_core::RunId,
) -> RunDetailInfo {
    // Convert chat messages
    let messages: Vec<RunMessage> = state
        .run_chat
        .get(run_id)
        .map(|msgs| {
            msgs.iter()
                .map(|msg| RunMessage {
                    role: match msg.role {
                        ChatRole::User => MessageRole::User,
                        ChatRole::Assistant => MessageRole::Assistant,
                        ChatRole::System => MessageRole::User, // System messages shown as user
                    },
                    content: msg.content.clone(),
                    timestamp: msg.timestamp,
                })
                .collect()
        })
        .unwrap_or_default();

    // Server doesn't track run events, so we provide empty events
    let events: Vec<RunEvent> = Vec::new();

    // Convert status from task/run status
    let status = match task.latest_run_status {
        Some(RunStatus::Running) => RunDetailStatus::Running,
        Some(RunStatus::Completed) => RunDetailStatus::Completed,
        Some(RunStatus::Failed) | Some(RunStatus::Cancelled) => RunDetailStatus::Failed,
        Some(RunStatus::Pending) | Some(RunStatus::Assigned) | None => {
            // Map task status if no run status
            match task.status {
                TaskStatus::Running => RunDetailStatus::Running,
                TaskStatus::Completed => RunDetailStatus::Completed,
                TaskStatus::Failed | TaskStatus::Cancelled => RunDetailStatus::Failed,
                TaskStatus::Pending => RunDetailStatus::Running,
            }
        }
    };

    // Get streaming output if any
    let current_output = state
        .run_output
        .get(run_id)
        .cloned()
        .unwrap_or_default();

    RunDetailInfo {
        run_id: run_id.to_string(),
        task_id: task.task_id.to_string(),
        agent: task.agent_name.clone(),
        status,
        started_at: task.created_at,
        completed_at: None, // Server doesn't track completion time
        messages,
        events,
        current_output,
        queued_input: None, // Server doesn't queue inputs
    }
}
