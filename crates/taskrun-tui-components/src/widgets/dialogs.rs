//! Dialog widgets for confirmations and inputs.

use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::theme::Theme;

/// Create a centered rectangle within the given area.
pub fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    Rect::new(x, y, width.min(area.width), height.min(area.height))
}

/// A simple confirmation dialog.
#[derive(Debug, Clone)]
pub struct ConfirmDialog<'a> {
    /// Dialog title.
    title: &'a str,
    /// Main message.
    message: &'a str,
    /// Optional secondary message.
    secondary: Option<&'a str>,
    /// Width of the dialog.
    width: u16,
    /// Height of the dialog.
    height: u16,
    /// Theme for styling.
    theme: Theme,
}

impl<'a> ConfirmDialog<'a> {
    /// Create a new confirmation dialog.
    pub fn new(title: &'a str, message: &'a str) -> Self {
        Self {
            title,
            message,
            secondary: None,
            width: 40,
            height: 7,
            theme: Theme::default(),
        }
    }

    /// Set a secondary message.
    pub fn secondary(mut self, message: &'a str) -> Self {
        self.secondary = Some(message);
        self.height = 9;
        self
    }

    /// Set the dialog size.
    pub fn size(mut self, width: u16, height: u16) -> Self {
        self.width = width;
        self.height = height;
        self
    }

    /// Set the theme.
    pub fn theme(mut self, theme: Theme) -> Self {
        self.theme = theme;
        self
    }

    /// Render the dialog.
    pub fn render(self, frame: &mut Frame) {
        let area = centered_rect(self.width, self.height, frame.area());

        frame.render_widget(Clear, area);

        let mut lines = vec![Line::from(""), Line::from(self.message)];

        if let Some(secondary) = self.secondary {
            lines.push(Line::from(""));
            lines.push(Line::from(secondary));
        }

        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled(
                "[Y]",
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("es  "),
            Span::styled(
                "[N]",
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            ),
            Span::raw("o"),
        ]));

        let paragraph = Paragraph::new(lines)
            .block(
                Block::default()
                    .title(format!(" {} ", self.title))
                    .borders(Borders::ALL)
                    .border_style(self.theme.focused_border()),
            )
            .alignment(Alignment::Center);

        frame.render_widget(paragraph, area);
    }
}

/// An input field widget.
#[derive(Debug, Clone)]
pub struct InputField {
    /// Current value.
    value: String,
    /// Cursor position.
    cursor: usize,
    /// Whether the field is focused.
    focused: bool,
    /// Placeholder text.
    placeholder: Option<String>,
    /// Theme for styling.
    theme: Theme,
}

impl InputField {
    /// Create a new input field.
    pub fn new(value: impl Into<String>) -> Self {
        let value = value.into();
        let cursor = value.chars().count();
        Self {
            value,
            cursor,
            focused: false,
            placeholder: None,
            theme: Theme::default(),
        }
    }

    /// Set the cursor position.
    pub fn cursor(mut self, cursor: usize) -> Self {
        self.cursor = cursor;
        self
    }

    /// Set whether the field is focused.
    pub fn focused(mut self, focused: bool) -> Self {
        self.focused = focused;
        self
    }

    /// Set placeholder text.
    pub fn placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = Some(placeholder.into());
        self
    }

    /// Set the theme.
    pub fn theme(mut self, theme: Theme) -> Self {
        self.theme = theme;
        self
    }

    /// Render the field and return the display text.
    pub fn render_text(&self) -> String {
        if self.value.is_empty() && !self.focused {
            return self.placeholder.clone().unwrap_or_default();
        }

        if self.focused {
            let char_count = self.value.chars().count();
            let cursor_pos = self.cursor.min(char_count);
            let before: String = self.value.chars().take(cursor_pos).collect();
            let after: String = self.value.chars().skip(cursor_pos).collect();
            format!("{}|{}", before, after)
        } else {
            self.value.clone()
        }
    }

    /// Get the style for this field.
    pub fn style(&self) -> Style {
        if self.focused {
            Style::default().bg(Color::DarkGray)
        } else if self.value.is_empty() && self.placeholder.is_some() {
            self.theme.muted_style()
        } else {
            Style::default()
        }
    }
}

/// A text input dialog.
#[derive(Debug, Clone)]
pub struct InputDialog<'a> {
    /// Dialog title.
    title: &'a str,
    /// Prompt message.
    prompt: &'a str,
    /// Current input value.
    value: &'a str,
    /// Cursor position.
    cursor: usize,
    /// Width of the dialog.
    width: u16,
    /// Theme for styling.
    theme: Theme,
}

impl<'a> InputDialog<'a> {
    /// Create a new input dialog.
    pub fn new(title: &'a str, prompt: &'a str, value: &'a str) -> Self {
        Self {
            title,
            prompt,
            value,
            cursor: value.chars().count(),
            width: 60,
            theme: Theme::default(),
        }
    }

    /// Set the cursor position.
    pub fn cursor(mut self, cursor: usize) -> Self {
        self.cursor = cursor;
        self
    }

    /// Set the dialog width.
    pub fn width(mut self, width: u16) -> Self {
        self.width = width;
        self
    }

    /// Set the theme.
    pub fn theme(mut self, theme: Theme) -> Self {
        self.theme = theme;
        self
    }

    /// Render the dialog.
    pub fn render(self, frame: &mut Frame) {
        let area = centered_rect(self.width, 7, frame.area());

        frame.render_widget(Clear, area);

        // Build input line with cursor
        let char_count = self.value.chars().count();
        let cursor_pos = self.cursor.min(char_count);
        let before: String = self.value.chars().take(cursor_pos).collect();
        let after: String = self.value.chars().skip(cursor_pos).collect();
        let input_display = format!("  {}|{}", before, after);

        let lines = vec![
            Line::from(""),
            Line::from(Span::styled(
                format!("  {}", self.prompt),
                Style::default().add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(Span::styled(
                input_display,
                Style::default().fg(Color::White),
            )),
            Line::from(Span::styled(
                "  [Enter] Submit  [Esc] Cancel",
                self.theme.muted_style(),
            )),
        ];

        let paragraph = Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan))
                .title(format!(" {} ", self.title)),
        );

        frame.render_widget(paragraph, area);
    }
}
