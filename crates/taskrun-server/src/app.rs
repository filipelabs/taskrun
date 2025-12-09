//! Server TUI application.

use std::io;
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::Backend;
use tokio::sync::mpsc;

use crate::event::{LogLevel, ServerCommand, ServerUiEvent};
use crate::render::render;
use crate::state::{ServerStatus, ServerUiState, ServerView, TaskDisplayInfo, WorkerDisplayInfo};

/// Server TUI application.
pub struct ServerApp {
    ui_rx: mpsc::Receiver<ServerUiEvent>,
    cmd_tx: mpsc::Sender<ServerCommand>,
    state: ServerUiState,
    should_quit: bool,
}

impl ServerApp {
    pub fn new(ui_rx: mpsc::Receiver<ServerUiEvent>, cmd_tx: mpsc::Sender<ServerCommand>) -> Self {
        Self {
            ui_rx,
            cmd_tx,
            state: ServerUiState::new(),
            should_quit: false,
        }
    }

    pub fn run<B: Backend>(&mut self, terminal: &mut Terminal<B>) -> io::Result<()> {
        while !self.should_quit {
            // Process backend events (non-blocking)
            self.process_events();

            // Render
            terminal.draw(|f| render(f, &self.state))?;

            // Poll for keyboard input with timeout
            if event::poll(Duration::from_millis(50))? {
                if let Event::Key(key) = event::read()? {
                    self.handle_key(key.code, key.modifiers);
                }
            }
        }

        Ok(())
    }

    fn process_events(&mut self) {
        // Process all available events without blocking
        while let Ok(event) = self.ui_rx.try_recv() {
            self.apply_event(event);
        }
    }

    fn apply_event(&mut self, event: ServerUiEvent) {
        match event {
            ServerUiEvent::ServerStarted { grpc_addr, http_addr } => {
                self.state.server_status = ServerStatus::Running;
                self.state.grpc_addr = grpc_addr;
                self.state.http_addr = http_addr;
            }
            ServerUiEvent::ServerError { message } => {
                self.state.server_status = ServerStatus::Error;
                self.state.error_message = Some(message.clone());
                self.state.add_log(LogLevel::Error, message);
            }
            ServerUiEvent::WorkerConnected { worker_id, hostname, agents } => {
                let info = WorkerDisplayInfo {
                    worker_id: worker_id.clone(),
                    hostname,
                    agents,
                    status: taskrun_core::WorkerStatus::Idle,
                    active_runs: 0,
                    max_concurrent_runs: 0,
                    connected_at: chrono::Utc::now(),
                    last_heartbeat: chrono::Utc::now(),
                };
                self.state.workers.insert(worker_id.clone(), info);
                self.state.add_log(LogLevel::Info, format!("Worker connected: {}", worker_id));
            }
            ServerUiEvent::WorkerDisconnected { worker_id } => {
                self.state.workers.remove(&worker_id);
                self.state.add_log(LogLevel::Info, format!("Worker disconnected: {}", worker_id));
            }
            ServerUiEvent::WorkerHeartbeat { worker_id, status, active_runs, max_concurrent_runs } => {
                if let Some(worker) = self.state.workers.get_mut(&worker_id) {
                    worker.status = status;
                    worker.active_runs = active_runs;
                    worker.max_concurrent_runs = max_concurrent_runs;
                    worker.last_heartbeat = chrono::Utc::now();
                }
            }
            ServerUiEvent::TaskCreated { task_id, agent } => {
                let info = TaskDisplayInfo {
                    task_id: task_id.clone(),
                    agent_name: agent,
                    status: taskrun_core::TaskStatus::Pending,
                    created_at: chrono::Utc::now(),
                    run_count: 0,
                    latest_run_id: None,
                    latest_run_status: None,
                };
                self.state.tasks.insert(task_id.clone(), info);
                self.state.task_list.insert(0, task_id.clone()); // Most recent first
                self.state.total_tasks += 1;
                self.state.add_log(LogLevel::Info, format!("Task created: {}", task_id));
            }
            ServerUiEvent::TaskStatusChanged { task_id, status } => {
                if let Some(task) = self.state.tasks.get_mut(&task_id) {
                    task.status = status;
                    match status {
                        taskrun_core::TaskStatus::Completed => self.state.completed_tasks += 1,
                        taskrun_core::TaskStatus::Failed => self.state.failed_tasks += 1,
                        _ => {}
                    }
                }
            }
            ServerUiEvent::RunStatusChanged { run_id, task_id, status } => {
                if let Some(task) = self.state.tasks.get_mut(&task_id) {
                    task.run_count += 1;
                    task.latest_run_id = Some(run_id);
                    task.latest_run_status = Some(status);
                }
            }
            ServerUiEvent::RunOutputChunk { run_id, content } => {
                self.state.run_output
                    .entry(run_id)
                    .or_default()
                    .push_str(&content);
            }
            ServerUiEvent::LogMessage { level, message } => {
                self.state.add_log(level, message);
            }
        }
    }

    fn handle_key(&mut self, code: KeyCode, modifiers: KeyModifiers) {
        // Handle dialogs first
        if self.state.show_quit_confirm {
            self.handle_quit_confirm(code);
            return;
        }
        if self.state.show_new_task_dialog {
            self.handle_new_task_dialog(code);
            return;
        }
        if self.state.show_cancel_confirm {
            self.handle_cancel_confirm(code);
            return;
        }
        if self.state.show_disconnect_confirm {
            self.handle_disconnect_confirm(code);
            return;
        }

        // Global keys
        match code {
            KeyCode::Char('q') | KeyCode::Esc => {
                if self.state.current_view == ServerView::RunDetail {
                    // Go back to tasks view
                    self.state.current_view = ServerView::Tasks;
                    self.state.viewing_task_id = None;
                    self.state.run_scroll = 0;
                } else {
                    self.state.show_quit_confirm = true;
                }
            }
            KeyCode::Char('c') if modifiers.contains(KeyModifiers::CONTROL) => {
                self.state.show_quit_confirm = true;
            }
            KeyCode::Char('1') => self.state.current_view = ServerView::Workers,
            KeyCode::Char('2') => self.state.current_view = ServerView::Tasks,
            KeyCode::Char('3') => self.state.current_view = ServerView::Logs,
            KeyCode::Tab => {
                if self.state.current_view != ServerView::RunDetail {
                    self.state.current_view = self.state.current_view.next();
                }
            }
            KeyCode::BackTab => {
                if self.state.current_view != ServerView::RunDetail {
                    self.state.current_view = self.state.current_view.prev();
                }
            }
            _ => {
                // View-specific keys
                match self.state.current_view {
                    ServerView::Workers => self.handle_workers_key(code),
                    ServerView::Tasks => self.handle_tasks_key(code),
                    ServerView::Logs => self.handle_logs_key(code),
                    ServerView::RunDetail => self.handle_run_detail_key(code),
                }
            }
        }
    }

    fn handle_workers_key(&mut self, code: KeyCode) {
        let worker_count = self.state.workers.len();
        match code {
            KeyCode::Char('j') | KeyCode::Down => {
                if worker_count > 0 {
                    self.state.selected_worker_index =
                        (self.state.selected_worker_index + 1).min(worker_count - 1);
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if self.state.selected_worker_index > 0 {
                    self.state.selected_worker_index -= 1;
                }
            }
            KeyCode::Char('d') => {
                if self.state.get_selected_worker().is_some() {
                    self.state.show_disconnect_confirm = true;
                }
            }
            KeyCode::Char('g') => self.state.selected_worker_index = 0,
            KeyCode::Char('G') => {
                if worker_count > 0 {
                    self.state.selected_worker_index = worker_count - 1;
                }
            }
            _ => {}
        }
    }

    fn handle_tasks_key(&mut self, code: KeyCode) {
        let task_count = self.state.task_list.len();
        match code {
            KeyCode::Char('j') | KeyCode::Down => {
                if task_count > 0 {
                    self.state.selected_task_index =
                        (self.state.selected_task_index + 1).min(task_count - 1);
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if self.state.selected_task_index > 0 {
                    self.state.selected_task_index -= 1;
                }
            }
            KeyCode::Char('n') => {
                self.state.show_new_task_dialog = true;
                self.state.new_task_agent.clear();
                self.state.new_task_input.clear();
                self.state.new_task_cursor = 0;
                self.state.new_task_field = 0;
            }
            KeyCode::Char('c') => {
                if self.state.get_selected_task().is_some() {
                    self.state.show_cancel_confirm = true;
                }
            }
            KeyCode::Enter => {
                if let Some(task) = self.state.get_selected_task() {
                    self.state.viewing_task_id = Some(task.task_id.clone());
                    self.state.current_view = ServerView::RunDetail;
                    self.state.run_scroll = usize::MAX; // Auto-scroll to bottom
                }
            }
            KeyCode::Char('g') => self.state.selected_task_index = 0,
            KeyCode::Char('G') => {
                if task_count > 0 {
                    self.state.selected_task_index = task_count - 1;
                }
            }
            _ => {}
        }
    }

    fn handle_logs_key(&mut self, code: KeyCode) {
        let log_count = self.state.log_messages.len();
        match code {
            KeyCode::Char('j') | KeyCode::Down => {
                if log_count > 0 {
                    self.state.log_scroll = (self.state.log_scroll + 1).min(log_count.saturating_sub(1));
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if self.state.log_scroll > 0 {
                    self.state.log_scroll -= 1;
                }
            }
            KeyCode::Char('g') => self.state.log_scroll = 0,
            KeyCode::Char('G') => {
                if log_count > 0 {
                    self.state.log_scroll = log_count - 1;
                }
            }
            _ => {}
        }
    }

    fn handle_run_detail_key(&mut self, code: KeyCode) {
        match code {
            KeyCode::Char('j') | KeyCode::Down => {
                // Disable auto-scroll when manually scrolling
                if self.state.run_scroll == usize::MAX {
                    self.state.run_scroll = 0;
                }
                self.state.run_scroll = self.state.run_scroll.saturating_add(1);
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if self.state.run_scroll == usize::MAX {
                    self.state.run_scroll = 0;
                }
                self.state.run_scroll = self.state.run_scroll.saturating_sub(1);
            }
            KeyCode::Char('g') => self.state.run_scroll = 0,
            KeyCode::Char('G') => self.state.run_scroll = usize::MAX, // Auto-scroll to bottom
            _ => {}
        }
    }

    fn handle_quit_confirm(&mut self, code: KeyCode) {
        match code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                // Send shutdown command
                let _ = self.cmd_tx.blocking_send(ServerCommand::Shutdown);
                self.should_quit = true;
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                self.state.show_quit_confirm = false;
            }
            _ => {}
        }
    }

    fn handle_new_task_dialog(&mut self, code: KeyCode) {
        match code {
            KeyCode::Esc => {
                self.state.show_new_task_dialog = false;
            }
            KeyCode::Tab => {
                self.state.new_task_field = (self.state.new_task_field + 1) % 2;
                // Reset cursor to end of field
                self.state.new_task_cursor = if self.state.new_task_field == 0 {
                    self.state.new_task_agent.len()
                } else {
                    self.state.new_task_input.len()
                };
            }
            KeyCode::Enter => {
                // Submit task
                if !self.state.new_task_agent.is_empty() {
                    let input = if self.state.new_task_input.is_empty() {
                        "{}".to_string()
                    } else {
                        self.state.new_task_input.clone()
                    };
                    let _ = self.cmd_tx.blocking_send(ServerCommand::CreateTask {
                        agent_name: self.state.new_task_agent.clone(),
                        input_json: input,
                    });
                    self.state.show_new_task_dialog = false;
                }
            }
            KeyCode::Char(c) => {
                let field = if self.state.new_task_field == 0 {
                    &mut self.state.new_task_agent
                } else {
                    &mut self.state.new_task_input
                };
                // Insert at cursor position (handle unicode safely)
                let byte_pos = field
                    .char_indices()
                    .nth(self.state.new_task_cursor)
                    .map(|(i, _)| i)
                    .unwrap_or(field.len());
                field.insert(byte_pos, c);
                self.state.new_task_cursor += 1;
            }
            KeyCode::Backspace => {
                let field = if self.state.new_task_field == 0 {
                    &mut self.state.new_task_agent
                } else {
                    &mut self.state.new_task_input
                };
                if self.state.new_task_cursor > 0 {
                    // Find byte position of char before cursor
                    let char_count = field.chars().count();
                    if self.state.new_task_cursor <= char_count {
                        let byte_pos = field
                            .char_indices()
                            .nth(self.state.new_task_cursor - 1)
                            .map(|(i, _)| i);
                        if let Some(start) = byte_pos {
                            let end = field
                                .char_indices()
                                .nth(self.state.new_task_cursor)
                                .map(|(i, _)| i)
                                .unwrap_or(field.len());
                            field.replace_range(start..end, "");
                            self.state.new_task_cursor -= 1;
                        }
                    }
                }
            }
            KeyCode::Left => {
                if self.state.new_task_cursor > 0 {
                    self.state.new_task_cursor -= 1;
                }
            }
            KeyCode::Right => {
                let field = if self.state.new_task_field == 0 {
                    &self.state.new_task_agent
                } else {
                    &self.state.new_task_input
                };
                let char_count = field.chars().count();
                if self.state.new_task_cursor < char_count {
                    self.state.new_task_cursor += 1;
                }
            }
            _ => {}
        }
    }

    fn handle_cancel_confirm(&mut self, code: KeyCode) {
        match code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                if let Some(task) = self.state.get_selected_task() {
                    let _ = self.cmd_tx.blocking_send(ServerCommand::CancelTask {
                        task_id: task.task_id.clone(),
                    });
                }
                self.state.show_cancel_confirm = false;
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                self.state.show_cancel_confirm = false;
            }
            _ => {}
        }
    }

    fn handle_disconnect_confirm(&mut self, code: KeyCode) {
        match code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                if let Some(worker) = self.state.get_selected_worker() {
                    let _ = self.cmd_tx.blocking_send(ServerCommand::DisconnectWorker {
                        worker_id: worker.worker_id.clone(),
                    });
                }
                self.state.show_disconnect_confirm = false;
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                self.state.show_disconnect_confirm = false;
            }
            _ => {}
        }
    }
}
