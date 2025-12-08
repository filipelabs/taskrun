//! Application state and main event loop.

use std::time::Duration;

use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::DefaultTerminal;
use tokio::sync::mpsc;

use crate::event::{BackendCommand, ConnectionState, UiEvent};
use crate::state::{UiState, View};
use crate::ui;

/// Main application with UI state and channel handles.
pub struct App {
    /// Current UI state snapshot for rendering.
    state: UiState,

    /// Receiver for events from the backend.
    ui_rx: mpsc::Receiver<UiEvent>,

    /// Sender for commands to the backend.
    cmd_tx: mpsc::Sender<BackendCommand>,
}

impl App {
    /// Create a new application instance with channel handles.
    pub fn new(ui_rx: mpsc::Receiver<UiEvent>, cmd_tx: mpsc::Sender<BackendCommand>) -> Self {
        Self {
            state: UiState::default(),
            ui_rx,
            cmd_tx,
        }
    }

    /// Run the main event loop.
    ///
    /// This runs on the main thread and handles:
    /// - Drawing the UI
    /// - Processing keyboard input
    /// - Receiving updates from the backend
    pub fn run(&mut self, mut terminal: DefaultTerminal) -> std::io::Result<()> {
        loop {
            // Draw the UI
            terminal.draw(|frame| ui::render(frame, &self.state))?;

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

            if self.state.should_quit() {
                break;
            }
        }

        // Send quit command to backend
        let _ = self.cmd_tx.blocking_send(BackendCommand::Quit);

        Ok(())
    }

    /// Apply an event from the backend to the UI state.
    ///
    /// Returns true if the app should quit.
    fn apply_event(&mut self, event: UiEvent) -> bool {
        match event {
            UiEvent::Tick => {
                // Could be used for animations
            }
            UiEvent::WorkersUpdated(workers) => {
                self.state.workers = workers;
                self.state.consecutive_failures = 0;
                self.state.last_error = None;
                self.update_status();
            }
            UiEvent::TasksUpdated(tasks) => {
                self.state.tasks = tasks;
                self.state.consecutive_failures = 0;
                self.state.last_error = None;
                self.update_status();
            }
            UiEvent::Error(msg) => {
                self.state.last_error = Some(msg);
                self.update_status();
            }
            UiEvent::ConnectionStateChanged(new_state) => {
                self.state.connection_state = new_state;
                // Clear error when reconnecting successfully
                if matches!(self.state.connection_state, ConnectionState::Connected) {
                    self.state.last_error = None;
                }
                self.update_status();
            }
            UiEvent::Key(_key) => {
                // Key events are handled directly in run()
            }
            UiEvent::Quit => {
                return true;
            }
        }
        false
    }

    /// Update the status message based on current state.
    fn update_status(&mut self) {
        self.state.status_message = Some(match &self.state.connection_state {
            ConnectionState::Connecting => "Connecting...".to_string(),
            ConnectionState::Connected => {
                if let Some(ref error) = self.state.last_error {
                    format!("Connected (error: {})", error)
                } else {
                    format!(
                        "Connected | Workers: {} | Tasks: {}",
                        self.state.workers.len(),
                        self.state.tasks.len()
                    )
                }
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
            // Quit (only q at top level, Esc goes back from detail views)
            KeyCode::Char('q') => {
                return true;
            }

            // Escape - go back from detail views or quit from main views
            KeyCode::Esc => {
                match self.state.current_view {
                    View::WorkerDetail => {
                        self.state.current_view = View::Workers;
                    }
                    _ => {
                        return true;
                    }
                }
            }

            // View switching with number keys
            KeyCode::Char('1') => {
                self.state.current_view = View::Workers;
            }
            KeyCode::Char('2') => {
                self.state.current_view = View::Tasks;
            }
            KeyCode::Char('3') => {
                self.state.current_view = View::Runs;
            }
            KeyCode::Char('4') => {
                self.state.current_view = View::Trace;
            }

            // Tab navigation (skip WorkerDetail in tab cycle)
            KeyCode::Tab => {
                self.state.current_view = match self.state.current_view {
                    View::Workers | View::WorkerDetail => View::Tasks,
                    View::Tasks => View::Runs,
                    View::Runs => View::Trace,
                    View::Trace => View::Workers,
                };
            }
            KeyCode::BackTab => {
                self.state.current_view = match self.state.current_view {
                    View::Workers | View::WorkerDetail => View::Trace,
                    View::Tasks => View::Workers,
                    View::Runs => View::Tasks,
                    View::Trace => View::Runs,
                };
            }

            // Up/Down or j/k navigation
            KeyCode::Up | KeyCode::Char('k') => {
                match self.state.current_view {
                    View::Workers | View::WorkerDetail => {
                        self.state.select_prev_worker();
                    }
                    View::Tasks => {
                        self.state.select_prev_task();
                    }
                    _ => {}
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                match self.state.current_view {
                    View::Workers | View::WorkerDetail => {
                        self.state.select_next_worker();
                    }
                    View::Tasks => {
                        self.state.select_next_task();
                    }
                    _ => {}
                }
            }

            // Enter - view details
            KeyCode::Enter => {
                match self.state.current_view {
                    View::Workers => {
                        if self.state.selected_worker().is_some() {
                            self.state.current_view = View::WorkerDetail;
                        }
                    }
                    _ => {}
                }
            }

            // Refresh / Reconnect
            KeyCode::Char('r') => {
                match &self.state.connection_state {
                    ConnectionState::Disconnected { .. } => {
                        // Force immediate reconnect when disconnected
                        let _ = self.cmd_tx.blocking_send(BackendCommand::ForceReconnect);
                    }
                    ConnectionState::Connected => {
                        // Normal refresh when connected
                        let _ = self.cmd_tx.blocking_send(BackendCommand::RefreshWorkers);
                        let _ = self.cmd_tx.blocking_send(BackendCommand::RefreshTasks);
                    }
                    ConnectionState::Connecting => {
                        // Do nothing while connecting
                    }
                }
            }

            _ => {}
        }
        false
    }
}

