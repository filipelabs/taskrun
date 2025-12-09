//! Chat message types for conversation history.

/// Role of a message in the conversation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatRole {
    /// User message (input/prompt).
    User,
    /// Assistant message (response).
    Assistant,
    /// System message (instructions).
    System,
}

/// A message in the conversation history.
#[derive(Debug, Clone)]
pub struct ChatMessage {
    /// Role of this message.
    pub role: ChatRole,
    /// Message content.
    pub content: String,
    /// Unix timestamp (milliseconds) when message was created.
    pub timestamp_ms: i64,
}

impl ChatMessage {
    /// Create a new chat message.
    pub fn new(role: ChatRole, content: impl Into<String>) -> Self {
        Self {
            role,
            content: content.into(),
            timestamp_ms: chrono::Utc::now().timestamp_millis(),
        }
    }

    /// Create a user message.
    pub fn user(content: impl Into<String>) -> Self {
        Self::new(ChatRole::User, content)
    }

    /// Create an assistant message.
    pub fn assistant(content: impl Into<String>) -> Self {
        Self::new(ChatRole::Assistant, content)
    }

    /// Create a system message.
    pub fn system(content: impl Into<String>) -> Self {
        Self::new(ChatRole::System, content)
    }
}
