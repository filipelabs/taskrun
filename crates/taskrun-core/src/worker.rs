//! Worker information types.

use crate::{AgentSpec, WorkerId};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Information about a worker's capabilities.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkerInfo {
    /// Unique worker identifier.
    pub worker_id: WorkerId,

    /// Hostname of the worker machine.
    pub hostname: String,

    /// Worker binary version.
    pub version: String,

    /// Agents available on this worker.
    pub agents: Vec<AgentSpec>,

    /// Worker-level labels (region, hardware, tenant, etc.).
    pub labels: HashMap<String, String>,
}

impl WorkerInfo {
    /// Create a new WorkerInfo.
    pub fn new(worker_id: WorkerId, hostname: impl Into<String>) -> Self {
        Self {
            worker_id,
            hostname: hostname.into(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            agents: Vec::new(),
            labels: HashMap::new(),
        }
    }

    /// Check if this worker supports a given agent.
    pub fn supports_agent(&self, agent_name: &str) -> bool {
        self.agents.iter().any(|a| a.name == agent_name)
    }

    /// Get an agent by name.
    pub fn get_agent(&self, agent_name: &str) -> Option<&AgentSpec> {
        self.agents.iter().find(|a| a.name == agent_name)
    }

    /// Builder method to add an agent.
    pub fn with_agent(mut self, agent: AgentSpec) -> Self {
        self.agents.push(agent);
        self
    }

    /// Builder method to add a label.
    pub fn with_label(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.labels.insert(key.into(), value.into());
        self
    }
}
