//! Converters between proto types and domain types.

use crate::pb;
use taskrun_core::{
    AgentSpec, ModelBackend, RunStatus, TaskStatus, WorkerId, WorkerInfo, WorkerStatus,
};

// ============================================================================
// TaskStatus conversions
// ============================================================================

impl From<TaskStatus> for pb::TaskStatus {
    fn from(status: TaskStatus) -> Self {
        match status {
            TaskStatus::Pending => pb::TaskStatus::Pending,
            TaskStatus::Running => pb::TaskStatus::Running,
            TaskStatus::Completed => pb::TaskStatus::Completed,
            TaskStatus::Failed => pb::TaskStatus::Failed,
            TaskStatus::Cancelled => pb::TaskStatus::Cancelled,
        }
    }
}

impl From<pb::TaskStatus> for TaskStatus {
    fn from(status: pb::TaskStatus) -> Self {
        match status {
            pb::TaskStatus::Unspecified => TaskStatus::Pending,
            pb::TaskStatus::Pending => TaskStatus::Pending,
            pb::TaskStatus::Running => TaskStatus::Running,
            pb::TaskStatus::Completed => TaskStatus::Completed,
            pb::TaskStatus::Failed => TaskStatus::Failed,
            pb::TaskStatus::Cancelled => TaskStatus::Cancelled,
        }
    }
}

// ============================================================================
// RunStatus conversions
// ============================================================================

impl From<RunStatus> for pb::RunStatus {
    fn from(status: RunStatus) -> Self {
        match status {
            RunStatus::Pending => pb::RunStatus::Pending,
            RunStatus::Assigned => pb::RunStatus::Assigned,
            RunStatus::Running => pb::RunStatus::Running,
            RunStatus::Completed => pb::RunStatus::Completed,
            RunStatus::Failed => pb::RunStatus::Failed,
            RunStatus::Cancelled => pb::RunStatus::Cancelled,
        }
    }
}

impl From<pb::RunStatus> for RunStatus {
    fn from(status: pb::RunStatus) -> Self {
        match status {
            pb::RunStatus::Unspecified => RunStatus::Pending,
            pb::RunStatus::Pending => RunStatus::Pending,
            pb::RunStatus::Assigned => RunStatus::Assigned,
            pb::RunStatus::Running => RunStatus::Running,
            pb::RunStatus::Completed => RunStatus::Completed,
            pb::RunStatus::Failed => RunStatus::Failed,
            pb::RunStatus::Cancelled => RunStatus::Cancelled,
        }
    }
}

// ============================================================================
// WorkerStatus conversions
// ============================================================================

impl From<WorkerStatus> for pb::WorkerStatus {
    fn from(status: WorkerStatus) -> Self {
        match status {
            WorkerStatus::Idle => pb::WorkerStatus::Idle,
            WorkerStatus::Busy => pb::WorkerStatus::Busy,
            WorkerStatus::Draining => pb::WorkerStatus::Draining,
            WorkerStatus::Error => pb::WorkerStatus::Error,
        }
    }
}

impl From<pb::WorkerStatus> for WorkerStatus {
    fn from(status: pb::WorkerStatus) -> Self {
        match status {
            pb::WorkerStatus::Unspecified => WorkerStatus::Idle,
            pb::WorkerStatus::Idle => WorkerStatus::Idle,
            pb::WorkerStatus::Busy => WorkerStatus::Busy,
            pb::WorkerStatus::Draining => WorkerStatus::Draining,
            pb::WorkerStatus::Error => WorkerStatus::Error,
        }
    }
}

// ============================================================================
// ModelBackend conversions
// ============================================================================

impl From<ModelBackend> for pb::ModelBackend {
    fn from(backend: ModelBackend) -> Self {
        pb::ModelBackend {
            provider: backend.provider,
            model_name: backend.model_name,
            context_window: backend.context_window,
            supports_streaming: backend.supports_streaming,
            modalities: backend.modalities,
            tools: backend.tools,
            metadata: backend.metadata,
        }
    }
}

impl From<pb::ModelBackend> for ModelBackend {
    fn from(proto: pb::ModelBackend) -> Self {
        ModelBackend {
            provider: proto.provider,
            model_name: proto.model_name,
            context_window: proto.context_window,
            supports_streaming: proto.supports_streaming,
            modalities: proto.modalities,
            tools: proto.tools,
            metadata: proto.metadata,
        }
    }
}

// ============================================================================
// AgentSpec conversions
// ============================================================================

impl From<AgentSpec> for pb::AgentSpec {
    fn from(agent: AgentSpec) -> Self {
        pb::AgentSpec {
            name: agent.name,
            description: agent.description,
            labels: agent.labels,
            backends: agent.backends.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<pb::AgentSpec> for AgentSpec {
    fn from(proto: pb::AgentSpec) -> Self {
        AgentSpec {
            name: proto.name,
            description: proto.description,
            labels: proto.labels,
            backends: proto.backends.into_iter().map(Into::into).collect(),
        }
    }
}

// ============================================================================
// WorkerInfo conversions
// ============================================================================

impl From<WorkerInfo> for pb::WorkerInfo {
    fn from(info: WorkerInfo) -> Self {
        pb::WorkerInfo {
            worker_id: info.worker_id.into_inner(),
            hostname: info.hostname,
            version: info.version,
            agents: info.agents.into_iter().map(Into::into).collect(),
            labels: info.labels,
        }
    }
}

impl From<pb::WorkerInfo> for WorkerInfo {
    fn from(proto: pb::WorkerInfo) -> Self {
        WorkerInfo {
            worker_id: WorkerId::new(proto.worker_id),
            hostname: proto.hostname,
            version: proto.version,
            agents: proto.agents.into_iter().map(Into::into).collect(),
            labels: proto.labels,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_status_roundtrip() {
        let statuses = [
            TaskStatus::Pending,
            TaskStatus::Running,
            TaskStatus::Completed,
            TaskStatus::Failed,
            TaskStatus::Cancelled,
        ];

        for status in statuses {
            let proto: pb::TaskStatus = status.into();
            let back: TaskStatus = proto.into();
            assert_eq!(status, back);
        }
    }

    #[test]
    fn test_model_backend_roundtrip() {
        let backend = ModelBackend::new("anthropic", "claude-3-5-sonnet")
            .with_context_window(200_000)
            .with_modalities(vec!["text".to_string(), "vision".to_string()]);

        let proto: pb::ModelBackend = backend.clone().into();
        let back: ModelBackend = proto.into();

        assert_eq!(backend.provider, back.provider);
        assert_eq!(backend.model_name, back.model_name);
        assert_eq!(backend.context_window, back.context_window);
    }
}
