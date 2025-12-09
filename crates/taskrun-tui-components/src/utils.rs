//! Text utilities for TUI rendering.

use unicode_width::UnicodeWidthChar;

/// Wrap text to fit within a given width, handling unicode safely.
///
/// Returns a vector of lines, each fitting within the specified width.
pub fn wrap_text(text: &str, width: usize) -> Vec<String> {
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
            let ch_width = UnicodeWidthChar::width(ch).unwrap_or(1);

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

/// Wrap text with an indent prefix for continuation lines.
pub fn wrap_text_indented(text: &str, width: usize, indent: &str) -> Vec<String> {
    let effective_width = width.saturating_sub(indent.chars().count());

    if effective_width == 0 {
        return vec![format!("{}{}", indent, text)];
    }

    let mut lines = Vec::new();

    for line in text.lines() {
        if line.is_empty() {
            lines.push(indent.to_string());
            continue;
        }

        let chars: Vec<char> = line.chars().collect();
        let mut start = 0;

        while start < chars.len() {
            let remaining_chars = chars.len() - start;

            if remaining_chars <= effective_width {
                let remaining: String = chars[start..].iter().collect();
                lines.push(format!("{}{}", indent, remaining));
                break;
            }

            // Find a good break point (prefer space within effective_width)
            let end = start + effective_width;
            let search_range: String = chars[start..end].iter().collect();

            let break_offset = search_range.rfind(' ').unwrap_or(effective_width);
            let actual_end = start + break_offset;

            let chunk: String = chars[start..actual_end].iter().collect();
            lines.push(format!("{}{}", indent, chunk.trim_end()));

            // Skip past the space
            start = actual_end;
            while start < chars.len() && chars[start] == ' ' {
                start += 1;
            }
        }
    }

    if lines.is_empty() {
        lines.push(indent.to_string());
    }

    lines
}

/// Truncate a string to fit within a given width, adding ellipsis if needed.
pub fn truncate(text: &str, max_width: usize) -> String {
    if max_width < 3 {
        return text.chars().take(max_width).collect();
    }

    let mut width = 0;
    let mut result = String::new();

    for ch in text.chars() {
        let ch_width = UnicodeWidthChar::width(ch).unwrap_or(1);
        if width + ch_width > max_width - 3 {
            result.push_str("...");
            return result;
        }
        result.push(ch);
        width += ch_width;
    }

    result
}

/// Format a duration in human-readable form.
pub fn format_duration(seconds: i64) -> String {
    if seconds < 60 {
        format!("{}s", seconds)
    } else if seconds < 3600 {
        format!("{}m {}s", seconds / 60, seconds % 60)
    } else {
        format!("{}h {}m", seconds / 3600, (seconds % 3600) / 60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wrap_text() {
        let text = "Hello world";
        let wrapped = wrap_text(text, 5);
        assert_eq!(wrapped, vec!["Hello", " worl", "d"]);
    }

    #[test]
    fn test_wrap_text_empty() {
        let wrapped = wrap_text("", 10);
        assert_eq!(wrapped, vec![""]);
    }

    #[test]
    fn test_truncate() {
        assert_eq!(truncate("Hello world", 8), "Hello...");
        assert_eq!(truncate("Hi", 10), "Hi");
    }

    #[test]
    fn test_format_duration() {
        assert_eq!(format_duration(30), "30s");
        assert_eq!(format_duration(90), "1m 30s");
        assert_eq!(format_duration(3700), "1h 1m");
    }
}
