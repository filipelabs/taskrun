//! Application state and main event loop.

use std::time::Duration;

use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::DefaultTerminal;
use tokio::sync::mpsc;

use crate::event::{BackendCommand, UiEvent};
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
            state: UiState {
                status_message: Some("Connecting...".to_string()),
                ..Default::default()
            },
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
                self.state.is_connected = true;
                self.state.last_error = None;
                self.update_status();
            }
            UiEvent::TasksUpdated(tasks) => {
                self.state.tasks = tasks;
                self.state.is_connected = true;
                self.state.last_error = None;
                self.update_status();
            }
            UiEvent::Error(msg) => {
                self.state.last_error = Some(msg);
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
        if let Some(ref error) = self.state.last_error {
            self.state.status_message = Some(format!("Error: {}", error));
        } else if self.state.is_connected {
            self.state.status_message = Some(format!(
                "Connected | Workers: {} | Tasks: {}",
                self.state.workers.len(),
                self.state.tasks.len()
            ));
        } else {
            self.state.status_message = Some("Connecting...".to_string());
        }
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

            // View switching
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

            // Tab navigation
            KeyCode::Tab => {
                self.state.current_view = match self.state.current_view {
                    View::Workers => View::Tasks,
                    View::Tasks => View::Runs,
                    View::Runs => View::Trace,
                    View::Trace => View::Workers,
                };
            }
            KeyCode::BackTab => {
                self.state.current_view = match self.state.current_view {
                    View::Workers => View::Trace,
                    View::Tasks => View::Workers,
                    View::Runs => View::Tasks,
                    View::Trace => View::Runs,
                };
            }

            // Refresh
            KeyCode::Char('r') => {
                let _ = self.cmd_tx.blocking_send(BackendCommand::RefreshWorkers);
                let _ = self.cmd_tx.blocking_send(BackendCommand::RefreshTasks);
            }

            _ => {}
        }
        false
    }
}

impl UiState {
    /// Check if the app should quit (used for Ctrl+C handling).
    fn should_quit(&self) -> bool {
        false // Could be set by signal handler
    }
}
