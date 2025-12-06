//! Converters between proto types and domain types.

use crate::pb;
use chrono::{TimeZone, Utc};
use taskrun_core::{
    AgentSpec, ModelBackend, RunId, RunStatus, RunSummary, Task, TaskId, TaskStatus, WorkerId,
    WorkerInfo, WorkerStatus,
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

// ============================================================================
// Task conversions
// ============================================================================

impl From<Task> for pb::Task {
    fn from(task: Task) -> Self {
        pb::Task {
            id: task.id.into_inner(),
            agent_name: task.agent_name,
            input_json: task.input_json,
            status: pb::TaskStatus::from(task.status).into(),
            created_by: task.created_by,
            created_at_ms: task.created_at.timestamp_millis(),
            labels: task.labels,
            runs: task.runs.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<pb::Task> for Task {
    fn from(proto: pb::Task) -> Self {
        let status = pb::TaskStatus::try_from(proto.status)
            .unwrap_or(pb::TaskStatus::Unspecified)
            .into();

        Task {
            id: TaskId::new(proto.id),
            agent_name: proto.agent_name,
            input_json: proto.input_json,
            status,
            created_by: proto.created_by,
            created_at: Utc
                .timestamp_millis_opt(proto.created_at_ms)
                .single()
                .unwrap_or_else(Utc::now),
            labels: proto.labels,
            runs: proto.runs.into_iter().map(Into::into).collect(),
        }
    }
}

// ============================================================================
// RunSummary conversions
// ============================================================================

impl From<RunSummary> for pb::RunSummary {
    fn from(run: RunSummary) -> Self {
        pb::RunSummary {
            run_id: run.run_id.into_inner(),
            worker_id: run.worker_id.into_inner(),
            status: pb::RunStatus::from(run.status).into(),
            started_at_ms: run.started_at.map(|t| t.timestamp_millis()).unwrap_or(0),
            finished_at_ms: run.finished_at.map(|t| t.timestamp_millis()).unwrap_or(0),
            backend_used: run.backend_used.map(Into::into),
            error_message: run.error_message.unwrap_or_default(),
        }
    }
}

impl From<pb::RunSummary> for RunSummary {
    fn from(proto: pb::RunSummary) -> Self {
        let status = pb::RunStatus::try_from(proto.status)
            .unwrap_or(pb::RunStatus::Unspecified)
            .into();

        RunSummary {
            run_id: RunId::new(proto.run_id),
            worker_id: WorkerId::new(proto.worker_id),
            status,
            started_at: if proto.started_at_ms > 0 {
                Utc.timestamp_millis_opt(proto.started_at_ms).single()
            } else {
                None
            },
            finished_at: if proto.finished_at_ms > 0 {
                Utc.timestamp_millis_opt(proto.finished_at_ms).single()
            } else {
                None
            },
            backend_used: proto.backend_used.map(Into::into),
            error_message: if proto.error_message.is_empty() {
                None
            } else {
                Some(proto.error_message)
            },
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
