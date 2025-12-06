//! Worker configuration.

use taskrun_core::WorkerId;

/// Worker configuration.
pub struct Config {
    /// Control plane address.
    pub control_plane_addr: String,

    /// Worker ID.
    pub worker_id: WorkerId,

    /// Heartbeat interval in seconds.
    pub heartbeat_interval_secs: u64,

    /// Reconnection delay on connection loss (seconds).
    pub reconnect_delay_secs: u64,

    /// Maximum concurrent runs this worker can handle.
    pub max_concurrent_runs: u32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            control_plane_addr: "http://[::1]:50051".to_string(),
            worker_id: WorkerId::generate(),
            heartbeat_interval_secs: 15,
            reconnect_delay_secs: 5,
            max_concurrent_runs: 10,
        }
    }
}
