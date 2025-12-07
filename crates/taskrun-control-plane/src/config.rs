//! Control plane configuration.

/// Control plane configuration.
#[allow(dead_code)]
pub struct Config {
    /// gRPC server bind address.
    pub bind_addr: String,

    /// Expected heartbeat interval from workers (seconds).
    pub heartbeat_interval_secs: u64,

    /// Heartbeat timeout before considering worker dead (seconds).
    pub heartbeat_timeout_secs: u64,

    /// Path to TLS certificate file.
    pub tls_cert_path: String,

    /// Path to TLS private key file.
    pub tls_key_path: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            bind_addr: "[::1]:50051".to_string(),
            heartbeat_interval_secs: 15,
            heartbeat_timeout_secs: 45,
            tls_cert_path: "certs/server.crt".to_string(),
            tls_key_path: "certs/server.key".to_string(),
        }
    }
}
