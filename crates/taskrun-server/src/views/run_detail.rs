//! Run detail view.

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

use taskrun_core::TaskStatus;

use crate::state::ServerUiState;

pub fn render_run_detail_view(f: &mut Frame, state: &ServerUiState, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5), // Task info header
            Constraint::Min(0),    // Output
        ])
        .split(area);

    render_task_header(f, state, chunks[0]);
    render_output(f, state, chunks[1]);
}

fn render_task_header(f: &mut Frame, state: &ServerUiState, area: Rect) {
    let task = match state.get_viewing_task() {
        Some(t) => t,
        None => {
            let para = Paragraph::new("No task selected")
                .block(Block::default().borders(Borders::ALL).title(" Task "));
            f.render_widget(para, area);
            return;
        }
    };

    let status_style = match task.status {
        TaskStatus::Pending => Style::default().fg(Color::Yellow),
        TaskStatus::Running => Style::default().fg(Color::Cyan),
        TaskStatus::Completed => Style::default().fg(Color::Green),
        TaskStatus::Failed => Style::default().fg(Color::Red),
        TaskStatus::Cancelled => Style::default().fg(Color::DarkGray),
    };

    let created_str = task.created_at.format("%Y-%m-%d %H:%M:%S").to_string();

    let lines = vec![
        Line::from(vec![
            Span::styled("Task ID: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(task.task_id.to_string()),
        ]),
        Line::from(vec![
            Span::styled("Agent: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(&task.agent_name),
            Span::raw("   "),
            Span::styled("Status: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::styled(format!("{:?}", task.status), status_style),
        ]),
        Line::from(vec![
            Span::styled("Created: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(created_str),
            Span::raw("   "),
            Span::styled("Runs: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(format!("{}", task.run_count)),
        ]),
    ];

    let paragraph = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title(" Task Info "));

    f.render_widget(paragraph, area);
}

fn render_output(f: &mut Frame, state: &ServerUiState, area: Rect) {
    let task = match state.get_viewing_task() {
        Some(t) => t,
        None => return,
    };

    let run_id = match &task.latest_run_id {
        Some(id) => id,
        None => {
            let para = Paragraph::new("No runs yet")
                .block(Block::default().borders(Borders::ALL).title(" Output "));
            f.render_widget(para, area);
            return;
        }
    };

    let output = state.run_output.get(run_id).map(|s| s.as_str()).unwrap_or("");

    // Wrap text and calculate lines
    let inner_width = area.width.saturating_sub(2) as usize;
    let inner_height = area.height.saturating_sub(2) as usize;

    let wrapped_lines = wrap_text(output, inner_width);
    let line_count = wrapped_lines.len();

    // Calculate scroll position
    let scroll_offset = if state.run_scroll == usize::MAX {
        // Auto-scroll to bottom
        line_count.saturating_sub(inner_height)
    } else {
        state.run_scroll.min(line_count.saturating_sub(inner_height))
    };

    let visible_lines: Vec<Line> = wrapped_lines
        .into_iter()
        .skip(scroll_offset)
        .take(inner_height)
        .map(Line::from)
        .collect();

    let scroll_indicator = if line_count > inner_height {
        let start = scroll_offset + 1;
        let end = (scroll_offset + inner_height).min(line_count);
        format!(" [{}-{}/{}] ", start, end, line_count)
    } else {
        format!(" [{}] ", line_count)
    };

    let title = format!(" Output (Run: {}){}", &run_id.to_string()[..8], scroll_indicator);

    let paragraph = Paragraph::new(visible_lines)
        .block(Block::default().borders(Borders::ALL).title(title))
        .wrap(Wrap { trim: false });

    f.render_widget(paragraph, area);
}

/// Wrap text to fit within the given width, handling unicode safely.
fn wrap_text(text: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return vec![];
    }

    let mut lines = Vec::new();

    for line in text.lines() {
        if line.is_empty() {
            lines.push(String::new());
            continue;
        }

        let mut current_line = String::new();
        let mut current_width = 0;

        for ch in line.chars() {
            let ch_width = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(1);

            if current_width + ch_width > width && !current_line.is_empty() {
                lines.push(current_line);
                current_line = String::new();
                current_width = 0;
            }

            current_line.push(ch);
            current_width += ch_width;
        }

        if !current_line.is_empty() {
            lines.push(current_line);
        }
    }

    if lines.is_empty() {
        lines.push(String::new());
    }

    lines
}
