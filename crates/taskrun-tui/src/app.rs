//! Application state and main event loop.

use std::time::Duration;

use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::DefaultTerminal;

use crate::ui;

/// Main application state.
pub struct App {
    /// Whether the application should quit.
    pub should_quit: bool,

    /// Current view/tab.
    pub current_view: View,

    /// Status message to display.
    pub status_message: Option<String>,
}

/// Available views in the TUI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum View {
    #[default]
    Workers,
    Tasks,
    Runs,
    Trace,
}

impl App {
    /// Create a new application instance.
    pub fn new() -> Self {
        Self {
            should_quit: false,
            current_view: View::Workers,
            status_message: Some("Press 'q' to quit, '1-4' to switch views".to_string()),
        }
    }

    /// Run the main event loop.
    pub fn run(&mut self, mut terminal: DefaultTerminal) -> std::io::Result<()> {
        loop {
            // Draw the UI
            terminal.draw(|frame| ui::render(frame, self))?;

            // Handle events with a timeout for responsiveness
            if event::poll(Duration::from_millis(100))? {
                if let Event::Key(key) = event::read()? {
                    if key.kind == KeyEventKind::Press {
                        self.handle_key(key.code);
                    }
                }
            }

            if self.should_quit {
                break;
            }
        }

        Ok(())
    }

    /// Handle a key press.
    fn handle_key(&mut self, code: KeyCode) {
        match code {
            // Quit
            KeyCode::Char('q') | KeyCode::Esc => {
                self.should_quit = true;
            }

            // View switching
            KeyCode::Char('1') => {
                self.current_view = View::Workers;
                self.status_message = Some("Workers view".to_string());
            }
            KeyCode::Char('2') => {
                self.current_view = View::Tasks;
                self.status_message = Some("Tasks view".to_string());
            }
            KeyCode::Char('3') => {
                self.current_view = View::Runs;
                self.status_message = Some("Runs view".to_string());
            }
            KeyCode::Char('4') => {
                self.current_view = View::Trace;
                self.status_message = Some("Trace view".to_string());
            }

            // Tab navigation
            KeyCode::Tab => {
                self.current_view = match self.current_view {
                    View::Workers => View::Tasks,
                    View::Tasks => View::Runs,
                    View::Runs => View::Trace,
                    View::Trace => View::Workers,
                };
            }
            KeyCode::BackTab => {
                self.current_view = match self.current_view {
                    View::Workers => View::Trace,
                    View::Tasks => View::Workers,
                    View::Runs => View::Tasks,
                    View::Trace => View::Runs,
                };
            }

            _ => {}
        }
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}
