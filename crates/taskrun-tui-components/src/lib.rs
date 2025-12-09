//! Shared TUI components for TaskRun applications.
//!
//! This crate provides reusable UI components, widgets, and utilities
//! for building terminal user interfaces in the TaskRun ecosystem.
//!
//! # Architecture
//!
//! The crate is organized into:
//! - `widgets` - Reusable ratatui widgets (header, footer, table, chat, events, logs, dialogs)
//! - `theme` - Colors, styles, and visual constants
//! - `utils` - Text wrapping, formatting utilities
//!
//! # Usage
//!
//! Components are designed to be data-agnostic. Pass data through trait
//! implementations or simple structs rather than depending on domain types.

pub mod theme;
pub mod utils;
pub mod widgets;

pub use theme::Theme;
pub use utils::{format_duration, truncate, wrap_text, wrap_text_indented};
pub use widgets::chat::{ChatMessage, ChatRole, ChatWidget};
pub use widgets::dialogs::{centered_rect, ConfirmDialog, InputDialog, InputField};
pub use widgets::events::{EventInfo, EventsWidget};
pub use widgets::footer::Footer;
pub use widgets::header::{Header, HeaderStat, StatusIndicator};
pub use widgets::logs::{LogEntry, LogLevel, LogsWidget};
pub use widgets::run_detail::{
    DetailPane, MessageRole, RunDetailView, RunEvent, RunInfo as RunDetailInfo, RunMessage,
    RunStatus as RunDetailStatus,
};
pub use widgets::table::{DataTable, TableCell, TableColumn, TableRow};
