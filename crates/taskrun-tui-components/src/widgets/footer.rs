//! Footer widget for TUI applications.

use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::theme::Theme;

/// Footer widget displaying help text or status.
#[derive(Debug, Clone)]
pub struct Footer<'a> {
    /// Help text or status message.
    text: &'a str,
    /// Theme for styling.
    theme: Theme,
}

impl<'a> Footer<'a> {
    /// Create a new footer with help text.
    pub fn new(text: &'a str) -> Self {
        Self {
            text,
            theme: Theme::default(),
        }
    }

    /// Set the theme.
    pub fn theme(mut self, theme: Theme) -> Self {
        self.theme = theme;
        self
    }

    /// Render the footer.
    pub fn render(self, frame: &mut Frame, area: Rect) {
        let footer = Paragraph::new(self.text).style(Style::default().fg(self.theme.muted));
        frame.render_widget(footer, area);
    }
}
