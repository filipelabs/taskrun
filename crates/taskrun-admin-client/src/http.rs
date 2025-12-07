//! HTTP client for REST endpoints.

use serde::de::DeserializeOwned;
use tracing::debug;

use crate::error::ClientError;

/// HTTP client for REST API endpoints.
pub struct HttpClient {
    inner: reqwest::Client,
    base_url: String,
}

impl HttpClient {
    /// Create a new HTTP client.
    pub fn new(base_url: &str) -> Self {
        Self {
            inner: reqwest::Client::new(),
            base_url: base_url.trim_end_matches('/').to_string(),
        }
    }

    /// Check if the control plane is healthy.
    pub async fn health(&self) -> Result<bool, ClientError> {
        let url = format!("{}/health", self.base_url);
        debug!(url = %url, "Checking health");

        let response = self.inner.get(&url).send().await?;
        Ok(response.status().is_success())
    }

    /// Get JSON from an endpoint.
    pub async fn get_json<T: DeserializeOwned>(&self, path: &str) -> Result<T, ClientError> {
        let url = format!("{}{}", self.base_url, path);
        debug!(url = %url, "GET request");

        let response = self.inner.get(&url).send().await?;

        if !response.status().is_success() {
            return Err(ClientError::NotFound(format!(
                "HTTP {}: {}",
                response.status(),
                path
            )));
        }

        response
            .json()
            .await
            .map_err(|e| ClientError::Serialization(e.to_string()))
    }
}
