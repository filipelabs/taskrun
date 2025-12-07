//! Control plane configuration.

/// Control plane configuration.
#[allow(dead_code)]
pub struct Config {
    /// gRPC server bind address.
    pub bind_addr: String,

    /// HTTP server bind address for enrollment.
    pub http_bind_addr: String,

    /// Expected heartbeat interval from workers (seconds).
    pub heartbeat_interval_secs: u64,

    /// Heartbeat timeout before considering worker dead (seconds).
    pub heartbeat_timeout_secs: u64,

    /// Path to TLS certificate file.
    pub tls_cert_path: String,

    /// Path to TLS private key file.
    pub tls_key_path: String,

    /// Path to CA certificate file.
    pub ca_cert_path: String,

    /// Path to CA private key file.
    pub ca_key_path: String,

    /// Bootstrap token validity in hours.
    pub bootstrap_token_validity_hours: u64,

    /// Worker certificate validity in days.
    pub worker_cert_validity_days: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            bind_addr: "[::1]:50051".to_string(),
            http_bind_addr: "[::1]:50052".to_string(),
            heartbeat_interval_secs: 15,
            heartbeat_timeout_secs: 45,
            tls_cert_path: "certs/server.crt".to_string(),
            tls_key_path: "certs/server.key".to_string(),
            ca_cert_path: "certs/ca.crt".to_string(),
            ca_key_path: "certs/ca.key".to_string(),
            bootstrap_token_validity_hours: 1,
            worker_cert_validity_days: 7,
        }
    }
}
