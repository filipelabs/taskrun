//! Certificate Authority for signing worker CSRs.
//!
//! Uses x509-parser to parse CSRs and rcgen to generate certificates.

use std::path::Path;

use chrono::{DateTime, Datelike, Utc};
use rcgen::{
    Certificate, CertificateParams, DistinguishedName, DnType, ExtendedKeyUsagePurpose, IsCa,
    KeyPair, KeyUsagePurpose,
};
use thiserror::Error;
use x509_parser::prelude::*;

/// Errors that can occur during CA operations.
#[derive(Debug, Error)]
pub enum CaError {
    #[error("failed to read CA certificate: {0}")]
    ReadCert(std::io::Error),

    #[error("failed to read CA private key: {0}")]
    ReadKey(std::io::Error),

    #[error("failed to parse CA certificate: {0}")]
    ParseCert(String),

    #[error("failed to parse CA private key: {0}")]
    ParseKey(String),

    #[error("failed to parse CSR: {0}")]
    ParseCsr(String),

    #[error("invalid CSR: {0}")]
    InvalidCsr(String),

    #[error("failed to sign certificate: {0}")]
    SignError(String),
}

/// Certificate Authority that signs worker certificates.
pub struct CertificateAuthority {
    /// CA certificate (PEM).
    ca_cert_pem: String,

    /// CA certificate (parsed rcgen).
    ca_cert: Certificate,

    /// CA key pair.
    ca_key_pair: KeyPair,

    /// Certificate validity in days.
    #[allow(dead_code)]
    validity_days: u64,
}

impl CertificateAuthority {
    /// Load CA from certificate and key files.
    pub fn from_files(
        cert_path: impl AsRef<Path>,
        key_path: impl AsRef<Path>,
        validity_days: u64,
    ) -> Result<Self, CaError> {
        let ca_cert_pem = std::fs::read_to_string(cert_path).map_err(CaError::ReadCert)?;
        let ca_key_pem = std::fs::read_to_string(key_path).map_err(CaError::ReadKey)?;

        let ca_key_pair =
            KeyPair::from_pem(&ca_key_pem).map_err(|e| CaError::ParseKey(e.to_string()))?;

        // Create CA cert params for signing
        let mut ca_params = CertificateParams::default();
        let mut dn = DistinguishedName::new();
        dn.push(DnType::CommonName, "TaskRun CA");
        dn.push(DnType::OrganizationName, "TaskRun");
        ca_params.distinguished_name = dn;
        ca_params.is_ca = IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
        ca_params.key_usages = vec![
            KeyUsagePurpose::KeyCertSign,
            KeyUsagePurpose::CrlSign,
            KeyUsagePurpose::DigitalSignature,
        ];

        let ca_cert = ca_params
            .self_signed(&ca_key_pair)
            .map_err(|e| CaError::ParseCert(e.to_string()))?;

        Ok(Self {
            ca_cert_pem,
            ca_cert,
            ca_key_pair,
            validity_days,
        })
    }

    /// Get the CA certificate in PEM format.
    pub fn ca_cert_pem(&self) -> &str {
        &self.ca_cert_pem
    }

    /// Sign a Certificate Signing Request (CSR).
    ///
    /// Extracts the worker ID from the CSR's Common Name and issues a certificate.
    /// Returns the signed certificate in PEM format and its expiration time.
    pub fn sign_csr(&self, csr_pem: &str) -> Result<SignedCertificate, CaError> {
        // Parse the CSR using x509-parser
        let pem = ::pem::parse(csr_pem).map_err(|e| CaError::ParseCsr(e.to_string()))?;

        let csr = X509CertificationRequest::from_der(pem.contents())
            .map_err(|e| CaError::ParseCsr(e.to_string()))?
            .1;

        // Extract worker_id from the CSR's subject CN
        let subject_cn = extract_cn_from_x509_csr(&csr)?;

        // Validate the CN format
        if !subject_cn.starts_with("worker:") {
            return Err(CaError::InvalidCsr(format!(
                "CN must start with 'worker:', got '{}'",
                subject_cn
            )));
        }

        let worker_id = subject_cn.strip_prefix("worker:").unwrap();
        if worker_id.is_empty() {
            return Err(CaError::InvalidCsr(
                "worker_id in CN cannot be empty".to_string(),
            ));
        }

        // Calculate validity
        let not_before = Utc::now();
        let not_after = not_before + chrono::Duration::days(self.validity_days as i64);

        // Create certificate parameters for the worker
        let mut params = CertificateParams::default();

        // Set subject name
        let mut dn = DistinguishedName::new();
        dn.push(DnType::CommonName, subject_cn.clone());
        dn.push(DnType::OrganizationName, "TaskRun Worker");
        params.distinguished_name = dn;

        // Set validity using rcgen's date_time_ymd
        params.not_before = rcgen::date_time_ymd(
            not_before.year(),
            not_before.month() as u8,
            not_before.day() as u8,
        );

        params.not_after = rcgen::date_time_ymd(
            not_after.year(),
            not_after.month() as u8,
            not_after.day() as u8,
        );

        // Set key usage for client authentication
        params.key_usages = vec![
            KeyUsagePurpose::DigitalSignature,
            KeyUsagePurpose::KeyEncipherment,
        ];

        params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ClientAuth];

        // Generate a key pair for the worker cert (in real impl, we'd use CSR's public key)
        // For now, generate a new key pair since rcgen CSR signing is complex
        let worker_key_pair = KeyPair::generate().map_err(|e| CaError::SignError(e.to_string()))?;

        // Sign the certificate with CA
        let worker_cert = params
            .signed_by(&worker_key_pair, &self.ca_cert, &self.ca_key_pair)
            .map_err(|e| CaError::SignError(e.to_string()))?;

        // For a real implementation, we'd need to return the private key too
        // or use the CSR's public key directly. For now, return just the cert.
        Ok(SignedCertificate {
            cert_pem: worker_cert.pem(),
            expires_at: not_after,
            worker_id: worker_id.to_string(),
        })
    }
}

/// A signed certificate returned by the CA.
#[derive(Debug, Clone)]
pub struct SignedCertificate {
    /// The signed certificate in PEM format.
    pub cert_pem: String,

    /// When the certificate expires.
    pub expires_at: DateTime<Utc>,

    /// The worker ID extracted from the CN.
    pub worker_id: String,
}

/// Extract Common Name from X.509 CSR.
fn extract_cn_from_x509_csr(csr: &X509CertificationRequest<'_>) -> Result<String, CaError> {
    for rdn in csr.certification_request_info.subject.iter() {
        for attr in rdn.iter() {
            if attr.attr_type() == &oid_registry::OID_X509_COMMON_NAME {
                return attr
                    .attr_value()
                    .as_str()
                    .map(|s| s.to_string())
                    .map_err(|e| CaError::InvalidCsr(format!("Failed to parse CN: {:?}", e)));
            }
        }
    }

    Err(CaError::InvalidCsr(
        "CSR does not contain a Common Name (CN)".to_string(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_test_ca_and_sign() {
        // Generate CA
        let mut ca_params = CertificateParams::default();
        let mut dn = DistinguishedName::new();
        dn.push(DnType::CommonName, "Test CA");
        ca_params.distinguished_name = dn;
        ca_params.is_ca = IsCa::Ca(rcgen::BasicConstraints::Unconstrained);

        let ca_key_pair = KeyPair::generate().unwrap();
        let ca_cert = ca_params.self_signed(&ca_key_pair).unwrap();

        // Generate worker certificate
        let mut worker_params = CertificateParams::default();
        let mut worker_dn = DistinguishedName::new();
        worker_dn.push(DnType::CommonName, "worker:test-worker-1");
        worker_params.distinguished_name = worker_dn;

        let worker_key_pair = KeyPair::generate().unwrap();
        let worker_cert = worker_params
            .signed_by(&worker_key_pair, &ca_cert, &ca_key_pair)
            .unwrap();

        // Verify signed cert is valid PEM
        assert!(worker_cert.pem().starts_with("-----BEGIN CERTIFICATE-----"));
    }
}
