//! Worker TUI application and main event loop.

use std::error::Error;
use std::time::Duration;

use chrono::Utc;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::DefaultTerminal;
use tokio::sync::mpsc;

use super::backend::run_worker_backend;
use super::connection::ConnectionConfig;
use super::event::{WorkerCommand, WorkerUiEvent};
use super::render;
use super::setup::{render_setup, SetupState};
use super::state::{
    ConnectionState, LogLevel, RunInfo, RunStatus, WorkerConfig, WorkerUiState, WorkerView,
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
                        KeyCode::Esc => {
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
                    if key.kind == KeyEventKind::Press && self.handle_key(key.code) {
                        break; // quit requested
                    }
                }
            }

            // Process backend events (non-blocking)
            while let Ok(event) = self.ui_rx.try_recv() {
                if self.apply_event(event) {
                    break; // quit requested
                }
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
            } => {
                let run = RunInfo {
                    run_id,
                    task_id,
                    agent,
                    status: RunStatus::Running,
                    started_at: Utc::now(),
                    completed_at: None,
                    output_preview: String::new(),
                };
                self.state.add_run(run);
                self.update_status();
            }
            WorkerUiEvent::RunProgress { run_id, output } => {
                // Update the run's output preview
                if let Some(run) = self.state.active_runs.iter_mut().find(|r| r.run_id == run_id) {
                    // Append to output preview (keep last 500 chars)
                    run.output_preview.push_str(&output);
                    if run.output_preview.len() > 500 {
                        let start = run.output_preview.len() - 500;
                        run.output_preview = run.output_preview[start..].to_string();
                    }
                }
            }
            WorkerUiEvent::RunCompleted {
                run_id,
                success,
                error_message,
            } => {
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
        match code {
            // Quit
            KeyCode::Char('q') | KeyCode::Esc => {
                return true;
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
}
