//! Logs view.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::event::LogLevel;
use crate::state::ServerUiState;

pub fn render_logs_view(f: &mut Frame, state: &ServerUiState, area: Rect) {
    let inner_height = area.height.saturating_sub(2) as usize;
    let log_count = state.log_messages.len();

    // Calculate scroll position
    let scroll_offset = if log_count <= inner_height {
        0
    } else {
        state.log_scroll.min(log_count.saturating_sub(inner_height))
    };

    let lines: Vec<Line> = state
        .log_messages
        .iter()
        .skip(scroll_offset)
        .take(inner_height)
        .map(|entry| {
            let level_style = match entry.level {
                LogLevel::Debug => Style::default().fg(Color::DarkGray),
                LogLevel::Info => Style::default().fg(Color::Blue),
                LogLevel::Warn => Style::default().fg(Color::Yellow),
                LogLevel::Error => Style::default().fg(Color::Red),
            };

            let timestamp = entry.timestamp.format("%H:%M:%S").to_string();

            Line::from(vec![
                Span::styled(timestamp, Style::default().fg(Color::DarkGray)),
                Span::raw(" "),
                Span::styled(format!("{:5}", entry.level.as_str()), level_style),
                Span::raw(" "),
                Span::raw(&entry.message),
            ])
        })
        .collect();

    let scroll_indicator = if log_count > inner_height {
        let start = scroll_offset + 1;
        let end = (scroll_offset + inner_height).min(log_count);
        format!(" [{}-{}/{}] ", start, end, log_count)
    } else {
        format!(" [{}] ", log_count)
    };

    let paragraph = Paragraph::new(lines).block(
        Block::default()
            .title(format!(" Logs{}", scroll_indicator))
            .borders(Borders::ALL),
    );

    f.render_widget(paragraph, area);
}
