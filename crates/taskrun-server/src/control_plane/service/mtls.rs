//! mTLS utilities for worker authentication.
//!
//! With mTLS enabled (client_ca_root in ServerTlsConfig), tonic automatically:
//! 1. Requires clients to present certificates
//! 2. Validates certificates are signed by our CA
//! 3. Rejects connections without valid certificates
//!
//! Our CA only signs certificates with CN="worker:<worker_id>", so any
//! connected worker is authenticated by virtue of having a valid certificate.

use taskrun_core::WorkerId;
use tonic::Status;

/// Validate that a worker_id follows the expected mTLS certificate format.
///
/// With mTLS, workers must have certificates signed by our CA, which only
/// issues certs with CN="worker:<worker_id>". This function validates that
/// the worker_id sent in WorkerHello matches the expected format.
///
/// # Security Note
/// The actual certificate validation is done by tonic's TLS layer with
/// `client_ca_root`. This function provides an additional format check
/// to ensure the worker_id is consistent with what would be in the cert.
#[allow(clippy::result_large_err)]
pub fn validate_worker_id_format(worker_id: &WorkerId) -> Result<(), Status> {
    let id_str = worker_id.as_str();

    // Worker ID should not be empty
    if id_str.is_empty() {
        return Err(Status::invalid_argument("worker_id cannot be empty"));
    }

    // Worker ID should be a reasonable format (alphanumeric, hyphens, underscores)
    if !id_str
        .chars()
        .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
    {
        return Err(Status::invalid_argument(
            "worker_id must contain only alphanumeric characters, hyphens, and underscores",
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_worker_ids() {
        assert!(validate_worker_id_format(&WorkerId::new("worker-1")).is_ok());
        assert!(validate_worker_id_format(&WorkerId::new("my_worker_123")).is_ok());
        assert!(validate_worker_id_format(&WorkerId::new("dev")).is_ok());
    }

    #[test]
    fn test_invalid_worker_ids() {
        // Empty
        assert!(validate_worker_id_format(&WorkerId::new("")).is_err());

        // Special characters
        assert!(validate_worker_id_format(&WorkerId::new("worker:1")).is_err());
        assert!(validate_worker_id_format(&WorkerId::new("worker/path")).is_err());
    }
}
