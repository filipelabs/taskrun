//! Theme and style definitions.

use ratatui::style::{Color, Modifier, Style};

/// Theme configuration for TaskRun TUI applications.
#[derive(Debug, Clone)]
pub struct Theme {
    /// Primary accent color (highlights, active elements)
    pub accent: Color,
    /// Success color (completed, connected)
    pub success: Color,
    /// Warning color (pending, in-progress)
    pub warning: Color,
    /// Error color (failed, disconnected)
    pub error: Color,
    /// Muted color (timestamps, secondary info)
    pub muted: Color,
    /// User message color
    pub user: Color,
    /// Assistant message color
    pub assistant: Color,
    /// System message color
    pub system: Color,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            accent: Color::Yellow,
            success: Color::Green,
            warning: Color::Yellow,
            error: Color::Red,
            muted: Color::DarkGray,
            user: Color::Cyan,
            assistant: Color::Green,
            system: Color::Yellow,
        }
    }
}

impl Theme {
    /// Style for focused/active borders.
    pub fn focused_border(&self) -> Style {
        Style::default().fg(self.accent)
    }

    /// Style for unfocused borders.
    pub fn unfocused_border(&self) -> Style {
        Style::default().fg(self.muted)
    }

    /// Style for success text.
    pub fn success_style(&self) -> Style {
        Style::default().fg(self.success)
    }

    /// Style for warning text.
    pub fn warning_style(&self) -> Style {
        Style::default().fg(self.warning)
    }

    /// Style for error text.
    pub fn error_style(&self) -> Style {
        Style::default().fg(self.error)
    }

    /// Style for muted/secondary text.
    pub fn muted_style(&self) -> Style {
        Style::default().fg(self.muted)
    }

    /// Style for bold text.
    pub fn bold(&self) -> Style {
        Style::default().add_modifier(Modifier::BOLD)
    }

    /// Style for user messages.
    pub fn user_style(&self) -> Style {
        Style::default().fg(self.user)
    }

    /// Style for assistant messages.
    pub fn assistant_style(&self) -> Style {
        Style::default().fg(self.assistant)
    }

    /// Style for system messages.
    pub fn system_style(&self) -> Style {
        Style::default().fg(self.system)
    }
}
