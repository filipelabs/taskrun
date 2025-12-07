//! Worker list handlers.

use std::sync::Arc;

use axum::{
    extract::State,
    http::header,
    response::IntoResponse,
    Json,
};

use crate::http::responses::{AgentResponse, BackendResponse, WorkerResponse};
use crate::state::AppState;

/// List workers as JSON.
pub async fn list_workers_json(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let workers = state.workers.read().await;
    let response: Vec<WorkerResponse> = workers
        .values()
        .map(|w| WorkerResponse {
            worker_id: w.info.worker_id.as_str().to_string(),
            hostname: w.info.hostname.clone(),
            version: w.info.version.clone(),
            status: format!("{:?}", w.status).to_uppercase(),
            active_runs: w.active_runs,
            max_concurrent_runs: w.max_concurrent_runs,
            last_heartbeat: w.last_heartbeat.to_rfc3339(),
            agents: w
                .info
                .agents
                .iter()
                .map(|a| AgentResponse {
                    name: a.name.clone(),
                    description: a.description.clone(),
                    backends: a
                        .backends
                        .iter()
                        .map(|b| BackendResponse {
                            provider: b.provider.clone(),
                            model_name: b.model_name.clone(),
                        })
                        .collect(),
                })
                .collect(),
        })
        .collect();
    Json(response)
}

/// List workers as HTML page.
pub async fn list_workers_html(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let workers = state.workers.read().await;
    let now = chrono::Utc::now();

    let mut rows = String::new();
    for worker in workers.values() {
        let status_color = match worker.status {
            taskrun_core::WorkerStatus::Idle => "#22c55e",
            taskrun_core::WorkerStatus::Busy => "#eab308",
            taskrun_core::WorkerStatus::Draining => "#f97316",
            taskrun_core::WorkerStatus::Error => "#ef4444",
        };

        let heartbeat_ago = format_relative_time(now, worker.last_heartbeat);

        let agents_html: Vec<String> = worker
            .info
            .agents
            .iter()
            .map(|a| {
                let models: Vec<String> = a
                    .backends
                    .iter()
                    .map(|b| format!("{}/{}", b.provider, b.model_name))
                    .collect();
                format!(
                    "<strong>{}</strong><br><small>{}</small>",
                    a.name,
                    models.join(", ")
                )
            })
            .collect();

        rows.push_str(&format!(
            r#"<tr>
                <td>{}</td>
                <td>{}</td>
                <td>{}</td>
                <td><span style="color: {}; font-weight: bold;">{:?}</span></td>
                <td>{}/{}</td>
                <td>{}</td>
                <td>{}</td>
            </tr>"#,
            worker.info.worker_id.as_str(),
            worker.info.hostname,
            worker.info.version,
            status_color,
            worker.status,
            worker.active_runs,
            worker.max_concurrent_runs,
            heartbeat_ago,
            agents_html.join("<hr style='margin:4px 0;border:none;border-top:1px solid #eee;'>")
        ));
    }

    if rows.is_empty() {
        rows = r#"<tr><td colspan="7" style="text-align:center;color:#666;">No workers connected</td></tr>"#.to_string();
    }

    let html = format!(
        r#"<!DOCTYPE html>
<html>
<head>
    <title>TaskRun Workers</title>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width, initial-scale=1">
    <style>
        body {{ font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; margin: 20px; background: #f5f5f5; }}
        h1 {{ color: #333; }}
        table {{ border-collapse: collapse; width: 100%; background: white; box-shadow: 0 1px 3px rgba(0,0,0,0.1); }}
        th, td {{ padding: 12px; text-align: left; border-bottom: 1px solid #eee; }}
        th {{ background: #f8f9fa; font-weight: 600; color: #555; }}
        tr:hover {{ background: #f8f9fa; }}
        small {{ color: #888; }}
        .refresh {{ color: #0066cc; text-decoration: none; margin-left: 20px; }}
        .refresh:hover {{ text-decoration: underline; }}
    </style>
</head>
<body>
    <h1>TaskRun Workers <a href="/ui/workers" class="refresh">↻ Refresh</a></h1>
    <p>Connected workers: <strong>{}</strong></p>
    <table>
        <thead>
            <tr>
                <th>Worker ID</th>
                <th>Hostname</th>
                <th>Version</th>
                <th>Status</th>
                <th>Runs</th>
                <th>Last Heartbeat</th>
                <th>Agents</th>
            </tr>
        </thead>
        <tbody>
            {}
        </tbody>
    </table>
    <p style="margin-top:20px;color:#888;font-size:12px;">
        JSON API: <a href="/v1/workers">/v1/workers</a> |
        Metrics: <a href="/metrics">/metrics</a>
    </p>
</body>
</html>"#,
        workers.len(),
        rows
    );

    ([(header::CONTENT_TYPE, "text/html; charset=utf-8")], html)
}

/// Format a duration as a human-readable relative time.
fn format_relative_time(now: chrono::DateTime<chrono::Utc>, then: chrono::DateTime<chrono::Utc>) -> String {
    let duration = now.signed_duration_since(then);
    if duration.num_seconds() < 60 {
        format!("{}s ago", duration.num_seconds())
    } else if duration.num_minutes() < 60 {
        format!("{}m ago", duration.num_minutes())
    } else {
        format!("{}h ago", duration.num_hours())
    }
}
