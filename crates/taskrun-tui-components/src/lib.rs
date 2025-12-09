//! Shared TUI components for TaskRun applications.
//!
//! This crate provides reusable UI components, widgets, and utilities
//! for building terminal user interfaces in the TaskRun ecosystem.
//!
//! # Architecture
//!
//! The crate is organized into:
//! - `widgets` - Reusable ratatui widgets (chat, events, logs, etc.)
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
pub use widgets::chat::{ChatMessage, ChatRole, ChatWidget};
pub use widgets::events::{EventInfo, EventsWidget};
