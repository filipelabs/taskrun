//! Model backend and agent specification types.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Represents a model backend available on a worker.
///
/// This is provider-agnostic and can represent any LLM backend
/// (Anthropic, OpenAI, Ollama, vLLM, local models, etc.).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelBackend {
    /// Provider name (e.g., "anthropic", "openai", "ollama", "vllm").
    pub provider: String,

    /// Model name (e.g., "claude-3-5-sonnet", "gpt-4o").
    pub model_name: String,

    /// Context window size in tokens.
    pub context_window: u32,

    /// Whether streaming output is supported.
    pub supports_streaming: bool,

    /// Supported modalities (e.g., "text", "vision", "audio").
    pub modalities: Vec<String>,

    /// Available tool/function names.
    pub tools: Vec<String>,

    /// Additional provider-specific metadata.
    pub metadata: HashMap<String, String>,
}

impl ModelBackend {
    /// Create a new ModelBackend with minimal required fields.
    pub fn new(provider: impl Into<String>, model_name: impl Into<String>) -> Self {
        Self {
            provider: provider.into(),
            model_name: model_name.into(),
            context_window: 0,
            supports_streaming: true,
            modalities: vec!["text".to_string()],
            tools: Vec::new(),
            metadata: HashMap::new(),
        }
    }

    /// Builder method to set context window.
    pub fn with_context_window(mut self, tokens: u32) -> Self {
        self.context_window = tokens;
        self
    }

    /// Builder method to set modalities.
    pub fn with_modalities(mut self, modalities: Vec<String>) -> Self {
        self.modalities = modalities;
        self
    }
}

/// Specification of an agent available on a worker.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentSpec {
    /// Unique agent name within the worker.
    pub name: String,

    /// Human-readable description.
    pub description: String,

    /// Agent-specific labels/tags.
    pub labels: HashMap<String, String>,

    /// Model backends this agent can use.
    pub backends: Vec<ModelBackend>,
}

impl AgentSpec {
    /// Create a new AgentSpec with minimal required fields.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: String::new(),
            labels: HashMap::new(),
            backends: Vec::new(),
        }
    }

    /// Builder method to set description.
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = desc.into();
        self
    }

    /// Builder method to add a backend.
    pub fn with_backend(mut self, backend: ModelBackend) -> Self {
        self.backends.push(backend);
        self
    }
}
