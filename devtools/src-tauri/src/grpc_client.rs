//! gRPC client for TaskService.
//!
//! Provides a client to communicate with the TaskRun control plane
//! using mTLS for secure connections.

use taskrun_proto::pb::*;
use taskrun_proto::TaskServiceClient;
use tonic::transport::{Certificate, Channel, ClientTlsConfig, Identity};

/// gRPC client wrapper for TaskService.
pub struct GrpcClient {
    client: TaskServiceClient<Channel>,
}

impl GrpcClient {
    /// Find the certificates directory by trying multiple possible paths.
    fn find_certs_dir() -> Option<std::path::PathBuf> {
        // Try paths relative to various possible working directories
        let candidates = [
            "certs",                     // From project root
            "../certs",                  // From devtools/
            "../../certs",               // From devtools/src-tauri/
            "../../../certs",            // From deeper paths
        ];

        for candidate in &candidates {
            let path = std::path::PathBuf::from(candidate);
            if path.join("ca.crt").exists() {
                return Some(path);
            }
        }
        None
    }

    /// Connect to the control plane using mTLS.
    ///
    /// Loads certificates from the project's `certs/` directory:
    /// - `ca.crt` - CA certificate for server verification
    /// - `worker.crt` - Client certificate for authentication
    /// - `worker.key` - Client private key
    pub async fn connect() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        // Find the certificates directory
        let certs_dir = Self::find_certs_dir()
            .ok_or_else(|| format!(
                "Could not find certs directory. CWD: {:?}",
                std::env::current_dir().ok()
            ))?;

        let ca_cert = std::fs::read(certs_dir.join("ca.crt"))?;
        let client_cert = std::fs::read(certs_dir.join("worker.crt"))?;
        let client_key = std::fs::read(certs_dir.join("worker.key"))?;

        let tls_config = ClientTlsConfig::new()
            .ca_certificate(Certificate::from_pem(ca_cert))
            .identity(Identity::from_pem(client_cert, client_key))
            .domain_name("localhost");

        let channel = Channel::from_static("https://[::1]:50051")
            .tls_config(tls_config)?
            .connect()
            .await?;

        let client = TaskServiceClient::new(channel);
        Ok(Self { client })
    }

    /// List tasks with optional limit.
    pub async fn list_tasks(&mut self, limit: i32) -> Result<Vec<Task>, String> {
        let request = ListTasksRequest {
            status_filter: 0, // No filter
            agent_filter: String::new(),
            limit,
        };
        let response = self
            .client
            .list_tasks(request)
            .await
            .map_err(|e| e.to_string())?;
        Ok(response.into_inner().tasks)
    }

    /// Create a new task.
    pub async fn create_task(
        &mut self,
        agent_name: String,
        input_json: String,
    ) -> Result<Task, String> {
        let request = CreateTaskRequest {
            agent_name,
            input_json,
            created_by: "devtools".to_string(),
            labels: Default::default(),
        };
        let response = self
            .client
            .create_task(request)
            .await
            .map_err(|e| e.to_string())?;
        Ok(response.into_inner())
    }

    /// Get a task by ID.
    pub async fn get_task(&mut self, id: String) -> Result<Task, String> {
        let request = GetTaskRequest { id };
        let response = self
            .client
            .get_task(request)
            .await
            .map_err(|e| e.to_string())?;
        Ok(response.into_inner())
    }

    /// Cancel a task by ID.
    pub async fn cancel_task(&mut self, id: String) -> Result<Task, String> {
        let request = CancelTaskRequest { id };
        let response = self
            .client
            .cancel_task(request)
            .await
            .map_err(|e| e.to_string())?;
        Ok(response.into_inner())
    }
}
