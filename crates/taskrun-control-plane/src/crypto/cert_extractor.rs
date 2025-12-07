//! Certificate parsing and worker ID extraction.
//!
//! Extracts worker_id from X.509 certificate CN field.
//! Expected format: "worker:<worker_id>"

use thiserror::Error;
use x509_parser::prelude::*;

/// Errors that can occur during certificate extraction.
#[derive(Debug, Error)]
pub enum CertExtractError {
    #[error("failed to parse certificate: {0}")]
    ParseError(String),

    #[error("certificate does not contain a Common Name (CN)")]
    MissingCn,

    #[error("CN must start with 'worker:', got '{0}'")]
    InvalidCnFormat(String),

    #[error("worker_id in CN cannot be empty")]
    EmptyWorkerId,
}

/// Extract worker_id from a DER-encoded X.509 certificate.
///
/// The certificate's Common Name (CN) must be in the format "worker:<worker_id>".
///
/// # Arguments
/// * `cert_der` - DER-encoded X.509 certificate bytes
///
/// # Returns
/// The worker_id extracted from the CN (without the "worker:" prefix).
pub fn extract_worker_id_from_cert(cert_der: &[u8]) -> Result<String, CertExtractError> {
    // Parse the X.509 certificate
    let (_, cert) = X509Certificate::from_der(cert_der)
        .map_err(|e| CertExtractError::ParseError(format!("{:?}", e)))?;

    // Extract CN from subject
    let cn = extract_cn_from_subject(&cert)?;

    // Validate format and extract worker_id
    if !cn.starts_with("worker:") {
        return Err(CertExtractError::InvalidCnFormat(cn));
    }

    let worker_id = cn.strip_prefix("worker:").unwrap();
    if worker_id.is_empty() {
        return Err(CertExtractError::EmptyWorkerId);
    }

    Ok(worker_id.to_string())
}

/// Extract Common Name from certificate subject.
fn extract_cn_from_subject(cert: &X509Certificate<'_>) -> Result<String, CertExtractError> {
    for rdn in cert.subject().iter() {
        for attr in rdn.iter() {
            if attr.attr_type() == &oid_registry::OID_X509_COMMON_NAME {
                return attr
                    .attr_value()
                    .as_str()
                    .map(|s| s.to_string())
                    .map_err(|e| CertExtractError::ParseError(format!("Failed to parse CN: {:?}", e)));
            }
        }
    }

    Err(CertExtractError::MissingCn)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rcgen::{CertificateParams, DistinguishedName, DnType, KeyPair};

    fn generate_test_cert(cn: &str) -> Vec<u8> {
        let mut params = CertificateParams::default();
        let mut dn = DistinguishedName::new();
        dn.push(DnType::CommonName, cn);
        params.distinguished_name = dn;

        let key_pair = KeyPair::generate().unwrap();
        let cert = params.self_signed(&key_pair).unwrap();
        cert.der().to_vec()
    }

    #[test]
    fn test_extract_valid_worker_id() {
        let cert_der = generate_test_cert("worker:test-worker-123");
        let result = extract_worker_id_from_cert(&cert_der);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "test-worker-123");
    }

    #[test]
    fn test_extract_invalid_cn_format() {
        let cert_der = generate_test_cert("not-a-worker");
        let result = extract_worker_id_from_cert(&cert_der);
        assert!(matches!(result, Err(CertExtractError::InvalidCnFormat(_))));
    }

    #[test]
    fn test_extract_empty_worker_id() {
        let cert_der = generate_test_cert("worker:");
        let result = extract_worker_id_from_cert(&cert_der);
        assert!(matches!(result, Err(CertExtractError::EmptyWorkerId)));
    }
}
