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
}

impl Default for Config {
    fn default() -> Self {
        Self {
            bind_addr: "[::1]:50051".to_string(),
            heartbeat_interval_secs: 15,
            heartbeat_timeout_secs: 45,
        }
    }
}
