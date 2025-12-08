//! Worker TUI application and main event loop.

use std::error::Error;
use std::time::Duration;

use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::DefaultTerminal;
use tokio::sync::mpsc;

use super::backend::run_worker_backend;
use super::connection::ConnectionConfig;
use super::event::{WorkerCommand, WorkerUiEvent};
use super::render;
use super::setup::{render_setup, SetupState};
use super::state::{
    ConnectionState, DetailPane, LogLevel, RunInfo, WorkerConfig, WorkerUiState, WorkerView,
};


/// Main entry point for the worker TUI.
pub fn run_worker_tui(config: WorkerConfig) -> Result<(), Box<dyn Error>> {
    // Initialize terminal (enters alternate screen, enables raw mode)
    let terminal = ratatui::init();

    // Run the app with setup phase first
    let result = run_app_with_setup(config, terminal);

    // Restore terminal (exits alternate screen, disables raw mode)
    ratatui::restore();

    result
}

/// Run the app, starting with setup if needed.
fn run_app_with_setup(
    mut config: WorkerConfig,
    mut terminal: DefaultTerminal,
) -> Result<(), Box<dyn Error>> {
    // Setup phase
    let mut setup_state = SetupState::default();

    // Pre-select based on config defaults
    setup_state.agent_index = super::setup::AGENT_OPTIONS
        .iter()
        .position(|(name, _)| *name == config.agent_name)
        .unwrap_or(0);
    setup_state.model_index = super::setup::MODEL_OPTIONS
        .iter()
        .position(|(name, _)| *name == config.model_name || config.model_name.contains(name))
        .unwrap_or(0);
    setup_state.agent_list_state.select(Some(setup_state.agent_index));
    setup_state.model_list_state.select(Some(setup_state.model_index));

    // Run setup loop
    loop {
        terminal.draw(|frame| render_setup(frame, &mut setup_state))?;

        if event::poll(Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Esc | KeyCode::Char('q') => {
                            // User cancelled
                            return Ok(());
                        }
                        _ => {
                            if setup_state.handle_key(key.code) {
                                // Setup complete, start the worker
                                break;
                            }
                        }
                    }
                }
            }
        }
    }

    // Update config with selected values
    config.agent_name = setup_state.selected_agent().to_string();
    config.model_name = setup_state.selected_model().to_string();

    // Now start the actual worker
    run_worker_app(config, terminal)
}

/// Run the worker app after setup is complete.
fn run_worker_app(config: WorkerConfig, mut terminal: DefaultTerminal) -> Result<(), Box<dyn Error>> {
    // Create channels for UI <-> backend communication
    let (ui_tx, ui_rx) = mpsc::channel::<WorkerUiEvent>(100);
    let (cmd_tx, cmd_rx) = mpsc::channel::<WorkerCommand>(100);

    // Generate worker ID once - used by both UI and backend
    let worker_id = ConnectionConfig::generate_worker_id();

    // Spawn background thread with its own tokio runtime
    let config_clone = config.clone();
    let worker_id_clone = worker_id.clone();
    let bg_handle = std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");
        rt.block_on(run_worker_backend(config_clone, worker_id_clone, ui_tx, cmd_rx));
    });

    // Run UI loop on main thread
    let mut app = WorkerApp::new(config, worker_id, ui_rx, cmd_tx);
    let result = app.run(&mut terminal);

    // Wait for background thread to finish
    let _ = bg_handle.join();

    result.map_err(|e| e.into())
}

/// Worker TUI application state and event loop.
pub struct WorkerApp {
    /// Current UI state.
    state: WorkerUiState,

    /// Receiver for events from the backend.
    ui_rx: mpsc::Receiver<WorkerUiEvent>,

    /// Sender for commands to the backend.
    cmd_tx: mpsc::Sender<WorkerCommand>,
}

impl WorkerApp {
    /// Create a new WorkerApp.
    pub fn new(
        config: WorkerConfig,
        worker_id: String,
        ui_rx: mpsc::Receiver<WorkerUiEvent>,
        cmd_tx: mpsc::Sender<WorkerCommand>,
    ) -> Self {
        Self {
            state: WorkerUiState::new(config, worker_id),
            ui_rx,
            cmd_tx,
        }
    }

    /// Run the main event loop.
    pub fn run(&mut self, terminal: &mut DefaultTerminal) -> std::io::Result<()> {
        loop {
            // Draw the UI
            terminal.draw(|frame| render::render(frame, &self.state))?;

            // Poll terminal events (non-blocking with short timeout)
            if event::poll(Duration::from_millis(50))? {
                if let Event::Key(key) = event::read()? {
                    if key.kind == KeyEventKind::Press {
                        if self.handle_key(key.code) {
                            break; // quit requested
                        }
                    }
                }
            }

            // Process backend events (non-blocking)
            let mut should_quit = false;
            while let Ok(event) = self.ui_rx.try_recv() {
                if self.apply_event(event) {
                    should_quit = true;
                    break;
                }
            }
            if should_quit {
                break; // quit requested from backend event
            }
        }

        // Send quit command to backend
        let _ = self.cmd_tx.blocking_send(WorkerCommand::Quit);

        Ok(())
    }

    /// Apply an event from the backend to the UI state.
    ///
    /// Returns true if the app should quit.
    fn apply_event(&mut self, event: WorkerUiEvent) -> bool {
        match event {
            WorkerUiEvent::Tick => {
                // Could be used for animations
            }
            WorkerUiEvent::Key(_key) => {
                // Key events are handled directly in run()
            }
            WorkerUiEvent::ConnectionStateChanged(new_state) => {
                self.state.connection_state = new_state;
                self.update_status();
            }
            WorkerUiEvent::RunStarted {
                run_id,
                task_id,
                agent,
                input,
            } => {
                let run = RunInfo::new(run_id, task_id, agent, input);
                self.state.add_run(run);
                self.update_status();
            }
            WorkerUiEvent::RunProgress { run_id, output } => {
                // Update the run's output (stored with 50KB cap)
                if let Some(run) = self.state.active_runs.iter_mut().find(|r| r.run_id == run_id) {
                    run.append_output(&output);
                }
                // Also update completed runs (for viewing history)
                if let Some(run) = self.state.completed_runs.iter_mut().find(|r| r.run_id == run_id) {
                    run.append_output(&output);
                }
            }
            WorkerUiEvent::RunEvent {
                run_id,
                event_type,
                details,
            } => {
                // Add event to the run
                if let Some(run) = self.state.active_runs.iter_mut().find(|r| r.run_id == run_id) {
                    run.add_event(event_type, details);
                } else if let Some(run) = self.state.completed_runs.iter_mut().find(|r| r.run_id == run_id)
                {
                    run.add_event(event_type, details);
                }
            }
            WorkerUiEvent::RunCompleted {
                run_id,
                success,
                error_message,
            } => {
                // Finalize streaming output as assistant message before completing
                if let Some(run) = self.state.active_runs.iter_mut().find(|r| r.run_id == run_id) {
                    run.finalize_output();
                }
                self.state.complete_run(&run_id, success);
                if let Some(error) = error_message {
                    self.state.add_log(LogLevel::Error, format!("Run {} failed: {}", run_id, error));
                }
                self.update_status();
            }
            WorkerUiEvent::LogMessage { level, message } => {
                self.state.add_log(level, message);
            }
            WorkerUiEvent::StatsUpdated { active_runs: _ } => {
                // Update active run count - already tracked via RunStarted/RunCompleted
                // This is a fallback for any discrepancy
            }
            WorkerUiEvent::SessionCaptured { run_id, session_id } => {
                // Store session_id in the run for continuation support
                if let Some(run) = self.state.active_runs.iter_mut().find(|r| r.run_id == run_id) {
                    run.session_id = Some(session_id.clone());
                }
                // Also check completed runs (session may arrive after completion)
                if let Some(run) = self.state.completed_runs.iter_mut().find(|r| r.run_id == run_id) {
                    run.session_id = Some(session_id);
                }
            }
            WorkerUiEvent::TurnCompleted { run_id } => {
                // Finalize current output as assistant message (for continuation turns)
                if let Some(run) = self.state.active_runs.iter_mut().find(|r| r.run_id == run_id) {
                    run.finalize_output();
                }
                if let Some(run) = self.state.completed_runs.iter_mut().find(|r| r.run_id == run_id) {
                    run.finalize_output();
                }
            }
            WorkerUiEvent::Quit => {
                return true;
            }
        }
        false
    }

    /// Update the status message based on current state.
    fn update_status(&mut self) {
        self.state.status_message = Some(match &self.state.connection_state {
            ConnectionState::Connecting => "Connecting to control plane...".to_string(),
            ConnectionState::Connected => {
                format!(
                    "Connected | Active: {} | Total: {} | Success: {} | Failed: {}",
                    self.state.active_runs.len(),
                    self.state.stats.total_runs,
                    self.state.stats.successful_runs,
                    self.state.stats.failed_runs
                )
            }
            ConnectionState::Disconnected { retry_in } => {
                format!(
                    "Disconnected - reconnecting in {}s (press 'r' to retry now)",
                    retry_in.as_secs()
                )
            }
        });
    }

    /// Handle a key press.
    ///
    /// Returns true if the app should quit.
    fn handle_key(&mut self, code: KeyCode) -> bool {
        // Handle quit confirmation dialog
        if self.state.show_quit_confirm {
            return self.handle_quit_confirm_key(code);
        }

        // Handle new run dialog
        if self.state.show_new_run_dialog {
            return self.handle_new_run_dialog_key(code);
        }

        // Handle detail view specially
        if self.state.current_view == WorkerView::RunDetail {
            return self.handle_detail_key(code);
        }

        match code {
            // Show quit confirmation
            KeyCode::Char('q') | KeyCode::Esc => {
                self.state.show_quit_confirm = true;
            }

            // New run (in Runs view)
            KeyCode::Char('n') => {
                if self.state.current_view == WorkerView::Runs {
                    self.state.show_new_run_dialog = true;
                    self.state.new_run_prompt.clear();
                    self.state.new_run_cursor = 0;
                }
            }

            // View switching with number keys
            KeyCode::Char('1') => {
                self.state.current_view = WorkerView::Status;
            }
            KeyCode::Char('2') => {
                self.state.current_view = WorkerView::Runs;
            }
            KeyCode::Char('3') => {
                self.state.current_view = WorkerView::Logs;
            }
            KeyCode::Char('4') => {
                self.state.current_view = WorkerView::Config;
            }

            // Tab navigation
            KeyCode::Tab => {
                self.state.current_view = self.state.current_view.next();
            }
            KeyCode::BackTab => {
                self.state.current_view = self.state.current_view.prev();
            }

            // Enter to select run (enter detail view)
            KeyCode::Enter => {
                if self.state.current_view == WorkerView::Runs {
                    self.state.enter_run_detail();
                }
            }

            // Up/Down or j/k navigation
            KeyCode::Up | KeyCode::Char('k') => {
                match self.state.current_view {
                    WorkerView::Runs => {
                        if self.state.selected_run_index > 0 {
                            self.state.selected_run_index -= 1;
                        }
                    }
                    WorkerView::Logs => {
                        if self.state.log_scroll_offset > 0 {
                            self.state.log_scroll_offset -= 1;
                        }
                    }
                    _ => {}
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                match self.state.current_view {
                    WorkerView::Runs => {
                        let total = self.state.active_runs.len() + self.state.completed_runs.len();
                        if self.state.selected_run_index < total.saturating_sub(1) {
                            self.state.selected_run_index += 1;
                        }
                    }
                    WorkerView::Logs => {
                        let max_scroll = self.state.log_messages.len().saturating_sub(20);
                        if self.state.log_scroll_offset < max_scroll {
                            self.state.log_scroll_offset += 1;
                        }
                    }
                    _ => {}
                }
            }

            // Reconnect
            KeyCode::Char('r') => {
                if matches!(self.state.connection_state, ConnectionState::Disconnected { .. }) {
                    let _ = self.cmd_tx.blocking_send(WorkerCommand::ForceReconnect);
                }
            }

            _ => {}
        }
        false
    }

    /// Handle key press in quit confirmation dialog.
    fn handle_quit_confirm_key(&mut self, code: KeyCode) -> bool {
        match code {
            // Confirm quit
            KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
                return true;
            }
            // Cancel quit
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                self.state.show_quit_confirm = false;
            }
            _ => {}
        }
        false
    }

    /// Handle key press in new run dialog.
    fn handle_new_run_dialog_key(&mut self, code: KeyCode) -> bool {
        match code {
            // Cancel
            KeyCode::Esc => {
                self.state.show_new_run_dialog = false;
                self.state.new_run_prompt.clear();
            }
            // Submit
            KeyCode::Enter => {
                if !self.state.new_run_prompt.is_empty() {
                    let prompt = self.state.new_run_prompt.clone();
                    self.state.show_new_run_dialog = false;
                    self.state.new_run_prompt.clear();
                    self.state.new_run_cursor = 0;

                    // Send command to create task
                    let _ = self.cmd_tx.blocking_send(WorkerCommand::CreateTask { prompt });
                    self.state.add_log(LogLevel::Info, "Creating new task...".to_string());
                }
            }
            // Character input (unicode-safe)
            KeyCode::Char(c) => {
                let byte_idx = self.state.new_run_prompt
                    .char_indices()
                    .nth(self.state.new_run_cursor)
                    .map(|(i, _)| i)
                    .unwrap_or(self.state.new_run_prompt.len());
                self.state.new_run_prompt.insert(byte_idx, c);
                self.state.new_run_cursor += 1;
            }
            // Backspace (unicode-safe)
            KeyCode::Backspace => {
                if self.state.new_run_cursor > 0 {
                    self.state.new_run_cursor -= 1;
                    if let Some((byte_idx, ch)) = self.state.new_run_prompt
                        .char_indices()
                        .nth(self.state.new_run_cursor)
                    {
                        self.state.new_run_prompt.replace_range(byte_idx..byte_idx + ch.len_utf8(), "");
                    }
                }
            }
            // Delete (unicode-safe)
            KeyCode::Delete => {
                let char_count = self.state.new_run_prompt.chars().count();
                if self.state.new_run_cursor < char_count {
                    if let Some((byte_idx, ch)) = self.state.new_run_prompt
                        .char_indices()
                        .nth(self.state.new_run_cursor)
                    {
                        self.state.new_run_prompt.replace_range(byte_idx..byte_idx + ch.len_utf8(), "");
                    }
                }
            }
            // Cursor movement (unicode-safe)
            KeyCode::Left => {
                if self.state.new_run_cursor > 0 {
                    self.state.new_run_cursor -= 1;
                }
            }
            KeyCode::Right => {
                let char_count = self.state.new_run_prompt.chars().count();
                if self.state.new_run_cursor < char_count {
                    self.state.new_run_cursor += 1;
                }
            }
            KeyCode::Home => {
                self.state.new_run_cursor = 0;
            }
            KeyCode::End => {
                self.state.new_run_cursor = self.state.new_run_prompt.chars().count();
            }
            _ => {}
        }
        false
    }

    /// Handle key press in detail view.
    fn handle_detail_key(&mut self, code: KeyCode) -> bool {
        // When input is focused, handle text input first
        if self.state.input_focused {
            match code {
                // Escape exits detail view
                KeyCode::Esc => {
                    self.state.exit_run_detail();
                    return false;
                }

                // Enter sends the message if session exists, or queues it
                KeyCode::Enter => {
                    if !self.state.chat_input.is_empty() {
                        let message = self.state.chat_input.clone();

                        // Check if we can send immediately (have session_id)
                        let can_send = self.state.get_viewing_run()
                            .map(|r| r.session_id.is_some())
                            .unwrap_or(false);

                        if can_send {
                            // Get run info for sending
                            if let Some(run) = self.state.get_viewing_run() {
                                let run_id = run.run_id.clone();
                                let session_id = run.session_id.clone().unwrap();

                                // Add user message to chat immediately
                                if let Some(run) = self.state.get_viewing_run_mut() {
                                    run.add_user_message(message.clone());
                                }

                                // Clear input
                                self.state.chat_input.clear();
                                self.state.chat_input_cursor = 0;

                                // Send the command
                                let _ = self.cmd_tx.blocking_send(WorkerCommand::ContinueRun {
                                    run_id,
                                    session_id: session_id.clone(),
                                    message,
                                });
                                self.state.add_log(
                                    LogLevel::Info,
                                    format!("Continuing session {}", &session_id[..8.min(session_id.len())]),
                                );
                            }
                        } else {
                            // No session yet - queue for later
                            self.state.queue_chat_message();
                            self.state.add_log(
                                LogLevel::Warn,
                                "No session ID yet - message queued".to_string(),
                            );
                        }
                    }
                }

                // Tab switches to events pane (unfocuses input)
                KeyCode::Tab => {
                    self.state.input_focused = false;
                    self.state.detail_pane = DetailPane::Events;
                }

                // Character input (unicode-safe)
                KeyCode::Char(c) => {
                    let byte_idx = self.state.chat_input
                        .char_indices()
                        .nth(self.state.chat_input_cursor)
                        .map(|(i, _)| i)
                        .unwrap_or(self.state.chat_input.len());
                    self.state.chat_input.insert(byte_idx, c);
                    self.state.chat_input_cursor += 1;
                }

                // Backspace (unicode-safe)
                KeyCode::Backspace => {
                    if self.state.chat_input_cursor > 0 {
                        self.state.chat_input_cursor -= 1;
                        if let Some((byte_idx, ch)) = self.state.chat_input
                            .char_indices()
                            .nth(self.state.chat_input_cursor)
                        {
                            self.state.chat_input.replace_range(byte_idx..byte_idx + ch.len_utf8(), "");
                        }
                    }
                }

                // Delete (unicode-safe)
                KeyCode::Delete => {
                    let char_count = self.state.chat_input.chars().count();
                    if self.state.chat_input_cursor < char_count {
                        if let Some((byte_idx, ch)) = self.state.chat_input
                            .char_indices()
                            .nth(self.state.chat_input_cursor)
                        {
                            self.state.chat_input.replace_range(byte_idx..byte_idx + ch.len_utf8(), "");
                        }
                    }
                }

                // Cursor movement (unicode-safe)
                KeyCode::Left => {
                    if self.state.chat_input_cursor > 0 {
                        self.state.chat_input_cursor -= 1;
                    }
                }
                KeyCode::Right => {
                    let char_count = self.state.chat_input.chars().count();
                    if self.state.chat_input_cursor < char_count {
                        self.state.chat_input_cursor += 1;
                    }
                }
                KeyCode::Home => {
                    self.state.chat_input_cursor = 0;
                }
                KeyCode::End => {
                    self.state.chat_input_cursor = self.state.chat_input.chars().count();
                }

                // Up arrow scrolls chat
                KeyCode::Up => {
                    if self.state.chat_scroll > 0 {
                        self.state.chat_scroll -= 1;
                    }
                }
                // Down arrow scrolls chat
                KeyCode::Down => {
                    self.state.chat_scroll += 1;
                }

                // Page up/down for chat scroll
                KeyCode::PageUp => {
                    self.state.chat_scroll = self.state.chat_scroll.saturating_sub(10);
                }
                KeyCode::PageDown => {
                    self.state.chat_scroll += 10;
                }

                _ => {}
            }
            return false;
        }

        // Input not focused - handle navigation
        match code {
            // Exit detail view
            KeyCode::Esc | KeyCode::Char('q') => {
                self.state.exit_run_detail();
            }

            // Switch pane / focus input
            KeyCode::Tab => {
                match self.state.detail_pane {
                    DetailPane::Output => {
                        self.state.detail_pane = DetailPane::Events;
                    }
                    DetailPane::Events => {
                        self.state.detail_pane = DetailPane::Output;
                        self.state.input_focused = true;
                    }
                }
            }

            // Enter focuses input (or 'i' like vim)
            KeyCode::Enter | KeyCode::Char('i') => {
                self.state.detail_pane = DetailPane::Output;
                self.state.input_focused = true;
            }

            // Scroll up
            KeyCode::Up | KeyCode::Char('k') => {
                match self.state.detail_pane {
                    DetailPane::Output => {
                        if self.state.chat_scroll > 0 {
                            self.state.chat_scroll -= 1;
                        }
                    }
                    DetailPane::Events => {
                        if self.state.events_scroll > 0 {
                            self.state.events_scroll -= 1;
                        }
                    }
                }
            }

            // Scroll down
            KeyCode::Down | KeyCode::Char('j') => {
                match self.state.detail_pane {
                    DetailPane::Output => {
                        self.state.chat_scroll += 1;
                    }
                    DetailPane::Events => {
                        self.state.events_scroll += 1;
                    }
                }
            }

            // Page up
            KeyCode::PageUp => {
                match self.state.detail_pane {
                    DetailPane::Output => {
                        self.state.chat_scroll = self.state.chat_scroll.saturating_sub(20);
                    }
                    DetailPane::Events => {
                        self.state.events_scroll = self.state.events_scroll.saturating_sub(20);
                    }
                }
            }

            // Page down
            KeyCode::PageDown => {
                match self.state.detail_pane {
                    DetailPane::Output => {
                        self.state.chat_scroll += 20;
                    }
                    DetailPane::Events => {
                        self.state.events_scroll += 20;
                    }
                }
            }

            // Home - scroll to top
            KeyCode::Home | KeyCode::Char('g') => {
                match self.state.detail_pane {
                    DetailPane::Output => {
                        self.state.chat_scroll = 0;
                    }
                    DetailPane::Events => {
                        self.state.events_scroll = 0;
                    }
                }
            }

            // End - scroll to bottom
            KeyCode::End | KeyCode::Char('G') => {
                match self.state.detail_pane {
                    DetailPane::Output => {
                        self.state.chat_scroll = usize::MAX / 2;
                    }
                    DetailPane::Events => {
                        self.state.events_scroll = usize::MAX / 2;
                    }
                }
            }

            _ => {}
        }
        false
    }
}
