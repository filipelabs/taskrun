//! Prometheus metrics collection and formatting.
//!
//! This module provides metrics in Prometheus text exposition format.

use std::fmt::Write;
use std::sync::Arc;

use taskrun_core::{TaskStatus, WorkerStatus};

use crate::state::AppState;

/// Collect all metrics from AppState and format as Prometheus text.
pub async fn collect_metrics(state: &Arc<AppState>) -> String {
    let mut output = String::new();

    collect_worker_metrics(state, &mut output).await;
    collect_task_metrics(state, &mut output).await;

    output
}

/// Collect worker metrics by status.
async fn collect_worker_metrics(state: &Arc<AppState>, output: &mut String) {
    let workers = state.workers.read().await;

    // Count workers by status
    let mut idle = 0u64;
    let mut busy = 0u64;
    let mut draining = 0u64;
    let mut error = 0u64;

    for worker in workers.values() {
        match worker.status {
            WorkerStatus::Idle => idle += 1,
            WorkerStatus::Busy => busy += 1,
            WorkerStatus::Draining => draining += 1,
            WorkerStatus::Error => error += 1,
        }
    }

    // Write Prometheus format
    writeln!(
        output,
        "# HELP taskrun_workers_connected Number of connected workers by status"
    )
    .ok();
    writeln!(output, "# TYPE taskrun_workers_connected gauge").ok();
    writeln!(output, "taskrun_workers_connected{{status=\"idle\"}} {idle}").ok();
    writeln!(output, "taskrun_workers_connected{{status=\"busy\"}} {busy}").ok();
    writeln!(
        output,
        "taskrun_workers_connected{{status=\"draining\"}} {draining}"
    )
    .ok();
    writeln!(
        output,
        "taskrun_workers_connected{{status=\"error\"}} {error}"
    )
    .ok();
}

/// Collect task metrics by status.
async fn collect_task_metrics(state: &Arc<AppState>, output: &mut String) {
    let tasks = state.tasks.read().await;

    // Count tasks by status
    let mut pending = 0u64;
    let mut running = 0u64;
    let mut completed = 0u64;
    let mut failed = 0u64;
    let mut cancelled = 0u64;

    for task in tasks.values() {
        match task.status {
            TaskStatus::Pending => pending += 1,
            TaskStatus::Running => running += 1,
            TaskStatus::Completed => completed += 1,
            TaskStatus::Failed => failed += 1,
            TaskStatus::Cancelled => cancelled += 1,
        }
    }

    // Write Prometheus format
    writeln!(output).ok();
    writeln!(
        output,
        "# HELP taskrun_tasks_total Total number of tasks by status"
    )
    .ok();
    writeln!(output, "# TYPE taskrun_tasks_total gauge").ok();
    writeln!(output, "taskrun_tasks_total{{status=\"pending\"}} {pending}").ok();
    writeln!(output, "taskrun_tasks_total{{status=\"running\"}} {running}").ok();
    writeln!(
        output,
        "taskrun_tasks_total{{status=\"completed\"}} {completed}"
    )
    .ok();
    writeln!(output, "taskrun_tasks_total{{status=\"failed\"}} {failed}").ok();
    writeln!(
        output,
        "taskrun_tasks_total{{status=\"cancelled\"}} {cancelled}"
    )
    .ok();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_collect_metrics_empty_state() {
        let state = AppState::new();
        let output = collect_metrics(&state).await;

        // Should contain worker metrics
        assert!(output.contains("taskrun_workers_connected"));
        assert!(output.contains("status=\"idle\""));

        // Should contain task metrics
        assert!(output.contains("taskrun_tasks_total"));
        assert!(output.contains("status=\"pending\""));

        // All counts should be 0
        assert!(output.contains("taskrun_workers_connected{status=\"idle\"} 0"));
        assert!(output.contains("taskrun_tasks_total{status=\"pending\"} 0"));
    }
}
