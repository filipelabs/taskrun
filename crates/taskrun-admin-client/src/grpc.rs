//! gRPC clients for TaskService and WorkerService.

use tonic::transport::{Certificate, Channel, ClientTlsConfig, Identity};
use tracing::info;

use taskrun_core::TaskId;
use taskrun_proto::pb::{
    CancelTaskRequest, CreateTaskRequest, GetTaskRequest, GetWorkerRequest, ListTasksRequest,
    ListWorkersRequest, Task, Worker,
};
use taskrun_proto::{TaskServiceClient, WorkerServiceClient};

use crate::error::ClientError;

/// Combined admin client with access to all services.
pub struct AdminClient {
    /// Task service client.
    pub tasks: TaskClient,
    /// Worker service client.
    pub workers: WorkerClient,
}

/// Client for the TaskService.
pub struct TaskClient {
    inner: TaskServiceClient<Channel>,
}

/// Client for the WorkerService.
pub struct WorkerClient {
    inner: WorkerServiceClient<Channel>,
}

impl AdminClient {
    /// Connect to the control plane.
    ///
    /// # Arguments
    /// * `endpoint` - The gRPC endpoint (e.g., "https://[::1]:50051")
    /// * `ca_cert` - Optional CA certificate for TLS
    /// * `client_identity` - Optional client cert+key tuple for mTLS
    pub async fn connect(
        endpoint: &str,
        ca_cert: Option<&[u8]>,
        client_identity: Option<&(Vec<u8>, Vec<u8>)>,
    ) -> Result<Self, ClientError> {
        info!(
            endpoint = %endpoint,
            tls = ca_cert.is_some(),
            mtls = client_identity.is_some(),
            "Connecting to control plane"
        );

        let channel = match (ca_cert, client_identity) {
            // Full mTLS: CA cert + client identity
            (Some(ca), Some((cert, key))) => {
                let tls = ClientTlsConfig::new()
                    .ca_certificate(Certificate::from_pem(ca))
                    .identity(Identity::from_pem(cert, key))
                    .domain_name("localhost");

                Channel::from_shared(endpoint.to_string())
                    .map_err(|e| ClientError::Connection(e.to_string()))?
                    .tls_config(tls)
                    .map_err(|e| ClientError::Connection(e.to_string()))?
                    .connect()
                    .await
                    .map_err(|e| ClientError::Connection(e.to_string()))?
            }
            // Server TLS only (no client cert)
            (Some(ca), None) => {
                let tls = ClientTlsConfig::new()
                    .ca_certificate(Certificate::from_pem(ca))
                    .domain_name("localhost");

                Channel::from_shared(endpoint.to_string())
                    .map_err(|e| ClientError::Connection(e.to_string()))?
                    .tls_config(tls)
                    .map_err(|e| ClientError::Connection(e.to_string()))?
                    .connect()
                    .await
                    .map_err(|e| ClientError::Connection(e.to_string()))?
            }
            // No TLS
            _ => Channel::from_shared(endpoint.to_string())
                .map_err(|e| ClientError::Connection(e.to_string()))?
                .connect()
                .await
                .map_err(|e| ClientError::Connection(e.to_string()))?,
        };

        Ok(Self {
            tasks: TaskClient {
                inner: TaskServiceClient::new(channel.clone()),
            },
            workers: WorkerClient {
                inner: WorkerServiceClient::new(channel),
            },
        })
    }
}

impl TaskClient {
    /// List all tasks.
    pub async fn list(&mut self) -> Result<Vec<Task>, ClientError> {
        let request = ListTasksRequest {
            status_filter: 0, // No filter
            agent_filter: String::new(),
            limit: 100,
        };
        let response = self.inner.list_tasks(request).await?;
        Ok(response.into_inner().tasks)
    }

    /// Get a specific task by ID.
    pub async fn get(&mut self, task_id: &TaskId) -> Result<Task, ClientError> {
        let request = GetTaskRequest {
            id: task_id.as_str().to_string(),
        };
        let response = self.inner.get_task(request).await?;
        Ok(response.into_inner())
    }

    /// Create a new task.
    pub async fn create(
        &mut self,
        agent_name: &str,
        input_json: &str,
        created_by: &str,
    ) -> Result<Task, ClientError> {
        let request = CreateTaskRequest {
            agent_name: agent_name.to_string(),
            input_json: input_json.to_string(),
            created_by: created_by.to_string(),
            labels: Default::default(),
        };
        let response = self.inner.create_task(request).await?;
        Ok(response.into_inner())
    }

    /// Cancel a task.
    pub async fn cancel(&mut self, task_id: &TaskId) -> Result<Task, ClientError> {
        let request = CancelTaskRequest {
            id: task_id.as_str().to_string(),
        };
        let response = self.inner.cancel_task(request).await?;
        Ok(response.into_inner())
    }
}

impl WorkerClient {
    /// List all workers.
    pub async fn list(&mut self) -> Result<Vec<Worker>, ClientError> {
        let request = ListWorkersRequest {
            agent_name: None,
            status: None,
        };
        let response = self.inner.list_workers(request).await?;
        Ok(response.into_inner().workers)
    }

    /// Get a specific worker by ID.
    pub async fn get(&mut self, worker_id: &str) -> Result<Worker, ClientError> {
        let request = GetWorkerRequest {
            worker_id: worker_id.to_string(),
        };
        let response = self.inner.get_worker(request).await?;
        Ok(response.into_inner())
    }
}
