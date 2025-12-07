//! Newtype wrappers for identifiers to ensure type safety.

use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

/// Unique identifier for a Task.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TaskId(String);

impl TaskId {
    /// Create a new TaskId from a string.
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Generate a new random TaskId.
    pub fn generate() -> Self {
        Self(Uuid::new_v4().to_string())
    }

    /// Get the inner string reference.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Consume and return the inner string.
    pub fn into_inner(self) -> String {
        self.0
    }
}

impl fmt::Display for TaskId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for TaskId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for TaskId {
    fn from(s: &str) -> Self {
        Self(s.to_owned())
    }
}

/// Unique identifier for a Run.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RunId(String);

impl RunId {
    /// Create a new RunId from a string.
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Generate a new random RunId.
    pub fn generate() -> Self {
        Self(Uuid::new_v4().to_string())
    }

    /// Get the inner string reference.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Consume and return the inner string.
    pub fn into_inner(self) -> String {
        self.0
    }
}

impl fmt::Display for RunId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for RunId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for RunId {
    fn from(s: &str) -> Self {
        Self(s.to_owned())
    }
}

/// Unique identifier for a Worker.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WorkerId(String);

/// Unique identifier for a RunEvent.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EventId(String);

impl EventId {
    /// Create a new EventId from a string.
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Generate a new random EventId.
    pub fn generate() -> Self {
        Self(Uuid::new_v4().to_string())
    }

    /// Get the inner string reference.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Consume and return the inner string.
    pub fn into_inner(self) -> String {
        self.0
    }
}

impl fmt::Display for EventId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for EventId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for EventId {
    fn from(s: &str) -> Self {
        Self(s.to_owned())
    }
}

impl WorkerId {
    /// Create a new WorkerId from a string.
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Generate a new random WorkerId.
    pub fn generate() -> Self {
        Self(Uuid::new_v4().to_string())
    }

    /// Get the inner string reference.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Consume and return the inner string.
    pub fn into_inner(self) -> String {
        self.0
    }
}

impl fmt::Display for WorkerId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for WorkerId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for WorkerId {
    fn from(s: &str) -> Self {
        Self(s.to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_id_generate() {
        let id1 = TaskId::generate();
        let id2 = TaskId::generate();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_id_display() {
        let id = TaskId::new("test-123");
        assert_eq!(format!("{}", id), "test-123");
    }
}
