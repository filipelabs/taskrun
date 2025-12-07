//! Type definitions for Claude Code control protocol messages.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Top-level message from Claude Code CLI stdout.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClaudeMessage {
    /// System initialization message.
    System {
        #[serde(default)]
        session_id: Option<String>,
        #[serde(default)]
        subtype: Option<String>,
        #[serde(default)]
        model: Option<String>,
        #[serde(default)]
        cwd: Option<String>,
    },

    /// Assistant response message.
    Assistant {
        message: AssistantMessage,
        #[serde(default)]
        session_id: Option<String>,
    },

    /// User message (echo).
    User {
        message: UserMessage,
        #[serde(default)]
        session_id: Option<String>,
    },

    /// Tool use notification.
    ToolUse {
        tool_name: String,
        #[serde(flatten)]
        tool_data: ToolData,
        #[serde(default)]
        session_id: Option<String>,
    },

    /// Tool result notification.
    ToolResult {
        result: Value,
        #[serde(default)]
        is_error: Option<bool>,
        #[serde(default)]
        session_id: Option<String>,
    },

    /// Streaming event.
    StreamEvent {
        event: StreamEvent,
        #[serde(default)]
        session_id: Option<String>,
    },

    /// Execution result.
    Result {
        #[serde(default, alias = "isError")]
        is_error: Option<bool>,
        #[serde(default, alias = "durationMs")]
        duration_ms: Option<u64>,
        #[serde(default)]
        result: Option<Value>,
        #[serde(default)]
        error: Option<String>,
        #[serde(default, alias = "sessionId")]
        session_id: Option<String>,
    },

    /// Control request from CLI (needs response).
    ControlRequest {
        request_id: String,
        request: ControlRequest,
    },

    /// Unknown message type (fallback).
    #[serde(untagged)]
    Unknown(Value),
}

impl ClaudeMessage {
    /// Extract session ID from any message type.
    pub fn session_id(&self) -> Option<&str> {
        match self {
            Self::System { session_id, .. } => session_id.as_deref(),
            Self::Assistant { session_id, .. } => session_id.as_deref(),
            Self::User { session_id, .. } => session_id.as_deref(),
            Self::ToolUse { session_id, .. } => session_id.as_deref(),
            Self::ToolResult { session_id, .. } => session_id.as_deref(),
            Self::StreamEvent { session_id, .. } => session_id.as_deref(),
            Self::Result { session_id, .. } => session_id.as_deref(),
            Self::ControlRequest { .. } | Self::Unknown(_) => None,
        }
    }
}

/// Assistant message content.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AssistantMessage {
    #[serde(default)]
    pub id: Option<String>,
    pub role: String,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub content: Vec<ContentItem>,
    #[serde(default)]
    pub stop_reason: Option<String>,
}

/// User message content.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct UserMessage {
    pub role: String,
    #[serde(default)]
    pub content: Vec<ContentItem>,
}

/// Content item in a message.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentItem {
    /// Text content.
    Text { text: String },

    /// Thinking/reasoning content.
    Thinking { thinking: String },

    /// Tool use request.
    ToolUse {
        id: String,
        #[serde(flatten)]
        tool_data: ToolData,
    },

    /// Tool result.
    ToolResult {
        tool_use_id: String,
        content: Value,
        #[serde(default)]
        is_error: Option<bool>,
    },
}

/// Structured tool data for Claude tools.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "name", content = "input")]
pub enum ToolData {
    /// Read a file.
    Read {
        #[serde(alias = "path")]
        file_path: String,
    },

    /// Write a file.
    Write {
        #[serde(alias = "path")]
        file_path: String,
        content: String,
    },

    /// Edit a file.
    Edit {
        #[serde(alias = "path")]
        file_path: String,
        #[serde(alias = "old_str")]
        old_string: Option<String>,
        #[serde(alias = "new_str")]
        new_string: Option<String>,
    },

    /// Execute a bash command.
    Bash {
        #[serde(alias = "cmd")]
        command: String,
        #[serde(default)]
        description: Option<String>,
    },

    /// Search with grep.
    Grep {
        pattern: String,
        #[serde(default)]
        path: Option<String>,
    },

    /// Find files with glob.
    Glob {
        pattern: String,
        #[serde(default)]
        path: Option<String>,
    },

    /// Spawn a sub-task.
    Task {
        #[serde(default)]
        subagent_type: Option<String>,
        #[serde(default)]
        description: Option<String>,
        #[serde(default)]
        prompt: Option<String>,
    },

    /// Unknown tool (fallback).
    #[serde(untagged)]
    Unknown(HashMap<String, Value>),
}

impl ToolData {
    /// Get the tool name.
    pub fn name(&self) -> &str {
        match self {
            Self::Read { .. } => "Read",
            Self::Write { .. } => "Write",
            Self::Edit { .. } => "Edit",
            Self::Bash { .. } => "Bash",
            Self::Grep { .. } => "Grep",
            Self::Glob { .. } => "Glob",
            Self::Task { .. } => "Task",
            Self::Unknown(data) => data
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown"),
        }
    }
}

/// Streaming event types.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StreamEvent {
    /// Message start.
    MessageStart { message: AssistantMessage },

    /// Content block start.
    ContentBlockStart {
        index: usize,
        content_block: ContentItem,
    },

    /// Content block delta (streaming text).
    ContentBlockDelta { index: usize, delta: ContentDelta },

    /// Content block stop.
    ContentBlockStop { index: usize },

    /// Message delta.
    MessageDelta {
        #[serde(default)]
        delta: Option<MessageDelta>,
    },

    /// Message stop.
    MessageStop,

    /// Unknown event.
    #[serde(other)]
    Unknown,
}

/// Content delta for streaming.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentDelta {
    /// Text delta.
    TextDelta { text: String },

    /// Thinking delta.
    ThinkingDelta { thinking: String },

    /// Unknown delta.
    #[serde(other)]
    Unknown,
}

/// Message delta for streaming.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct MessageDelta {
    #[serde(default)]
    pub stop_reason: Option<String>,
}

/// Control request from CLI to SDK.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "subtype", rename_all = "snake_case")]
pub enum ControlRequest {
    /// Permission check for tool use.
    CanUseTool {
        tool_name: String,
        input: Value,
        #[serde(default)]
        permission_suggestions: Option<Vec<PermissionUpdate>>,
    },

    /// Hook callback.
    HookCallback {
        callback_id: String,
        input: Value,
        #[serde(default)]
        tool_use_id: Option<String>,
    },
}

/// Permission result for tool use.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "behavior", rename_all = "camelCase")]
pub enum PermissionResult {
    /// Allow the tool use.
    Allow {
        #[serde(rename = "updatedInput")]
        updated_input: Value,
        #[serde(skip_serializing_if = "Option::is_none", rename = "updatedPermissions")]
        updated_permissions: Option<Vec<PermissionUpdate>>,
    },

    /// Deny the tool use.
    Deny {
        message: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        interrupt: Option<bool>,
    },
}

/// Permission update operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionUpdate {
    #[serde(rename = "type")]
    pub update_type: PermissionUpdateType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<PermissionMode>,
    pub destination: PermissionUpdateDestination,
}

/// Type of permission update.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PermissionUpdateType {
    SetMode,
    AddRules,
    RemoveRules,
    ClearRules,
}

/// Destination for permission update.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PermissionUpdateDestination {
    Session,
    UserSettings,
    ProjectSettings,
    LocalSettings,
}

/// Permission mode for Claude Code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PermissionMode {
    /// Default mode - asks for permission.
    Default,
    /// Accept file edits automatically.
    AcceptEdits,
    /// Plan mode - requires approval to exit.
    Plan,
    /// Bypass all permissions (dangerous).
    BypassPermissions,
}

impl Default for PermissionMode {
    fn default() -> Self {
        Self::Default
    }
}

impl std::fmt::Display for PermissionMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Default => write!(f, "default"),
            Self::AcceptEdits => write!(f, "acceptEdits"),
            Self::Plan => write!(f, "plan"),
            Self::BypassPermissions => write!(f, "bypassPermissions"),
        }
    }
}

/// Control request from SDK to CLI.
#[derive(Debug, Clone, Serialize)]
pub struct SdkControlRequest {
    #[serde(rename = "type")]
    pub message_type: String,
    pub request_id: String,
    pub request: SdkControlRequestType,
}

impl SdkControlRequest {
    pub fn new(request: SdkControlRequestType) -> Self {
        Self {
            message_type: "control_request".to_string(),
            request_id: uuid::Uuid::new_v4().to_string(),
            request,
        }
    }
}

/// Types of SDK control requests.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "subtype", rename_all = "snake_case")]
pub enum SdkControlRequestType {
    /// Initialize the control protocol.
    Initialize {
        #[serde(skip_serializing_if = "Option::is_none")]
        hooks: Option<Value>,
    },

    /// Set the permission mode.
    SetPermissionMode { mode: PermissionMode },
}

/// Control response from SDK to CLI.
#[derive(Debug, Clone, Serialize)]
pub struct ControlResponse {
    #[serde(rename = "type")]
    pub message_type: String,
    pub response: ControlResponseType,
}

impl ControlResponse {
    pub fn new(response: ControlResponseType) -> Self {
        Self {
            message_type: "control_response".to_string(),
            response,
        }
    }
}

/// Types of control responses.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "subtype", rename_all = "snake_case")]
pub enum ControlResponseType {
    /// Successful response.
    Success {
        request_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        response: Option<Value>,
    },

    /// Error response.
    Error {
        request_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_system_message_parsing() {
        let json = r#"{"type":"system","subtype":"init","session_id":"abc123","model":"claude-sonnet-4"}"#;
        let msg: ClaudeMessage = serde_json::from_str(json).unwrap();

        assert!(matches!(msg, ClaudeMessage::System { .. }));
        assert_eq!(msg.session_id(), Some("abc123"));
    }

    #[test]
    fn test_assistant_message_parsing() {
        let json = r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"Hello world"}]},"session_id":"test123"}"#;
        let msg: ClaudeMessage = serde_json::from_str(json).unwrap();

        if let ClaudeMessage::Assistant { message, .. } = msg {
            assert_eq!(message.role, "assistant");
            assert_eq!(message.content.len(), 1);
            if let ContentItem::Text { text } = &message.content[0] {
                assert_eq!(text, "Hello world");
            } else {
                panic!("Expected text content");
            }
        } else {
            panic!("Expected assistant message");
        }
    }

    #[test]
    fn test_tool_data_parsing() {
        let json = r#"{"name":"Read","input":{"file_path":"/tmp/test.rs"}}"#;
        let tool: ToolData = serde_json::from_str(json).unwrap();

        if let ToolData::Read { file_path } = tool {
            assert_eq!(file_path, "/tmp/test.rs");
        } else {
            panic!("Expected Read tool");
        }
    }

    #[test]
    fn test_bash_tool_parsing() {
        let json = r#"{"name":"Bash","input":{"command":"ls -la","description":"List files"}}"#;
        let tool: ToolData = serde_json::from_str(json).unwrap();

        if let ToolData::Bash {
            command,
            description,
        } = tool
        {
            assert_eq!(command, "ls -la");
            assert_eq!(description, Some("List files".to_string()));
        } else {
            panic!("Expected Bash tool");
        }
    }

    #[test]
    fn test_permission_mode_serialization() {
        assert_eq!(
            serde_json::to_string(&PermissionMode::BypassPermissions).unwrap(),
            r#""bypassPermissions""#
        );
        assert_eq!(
            serde_json::to_string(&PermissionMode::Default).unwrap(),
            r#""default""#
        );
    }

    #[test]
    fn test_control_request_parsing() {
        let json = r#"{"type":"control_request","request_id":"req-123","request":{"subtype":"can_use_tool","tool_name":"Bash","input":{"command":"ls"}}}"#;
        let msg: ClaudeMessage = serde_json::from_str(json).unwrap();

        if let ClaudeMessage::ControlRequest {
            request_id,
            request,
        } = msg
        {
            assert_eq!(request_id, "req-123");
            if let ControlRequest::CanUseTool {
                tool_name, input, ..
            } = request
            {
                assert_eq!(tool_name, "Bash");
                assert_eq!(input["command"], "ls");
            } else {
                panic!("Expected CanUseTool request");
            }
        } else {
            panic!("Expected control request");
        }
    }

    #[test]
    fn test_result_message_parsing() {
        let json = r#"{"type":"result","isError":false,"durationMs":1234,"sessionId":"sess-abc"}"#;
        let msg: ClaudeMessage = serde_json::from_str(json).unwrap();

        if let ClaudeMessage::Result {
            is_error,
            duration_ms,
            session_id,
            ..
        } = msg
        {
            assert_eq!(is_error, Some(false));
            assert_eq!(duration_ms, Some(1234));
            assert_eq!(session_id, Some("sess-abc".to_string()));
        } else {
            panic!("Expected result message");
        }
    }
}
