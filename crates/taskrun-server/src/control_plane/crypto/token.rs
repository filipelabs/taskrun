//! Bootstrap token generation and validation.
//!
//! Bootstrap tokens are used for initial worker enrollment.
//! They are single-use, time-limited tokens that allow workers
//! to request a certificate via CSR.

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use chrono::{DateTime, Duration, Utc};
use rand::RngCore;
use sha2::{Digest, Sha256};

/// A bootstrap token stored in the control plane.
/// We never store the plaintext token - only its SHA-256 hash.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct BootstrapToken {
    /// SHA-256 hash of the token (hex encoded).
    pub token_hash: String,

    /// When the token was created.
    pub created_at: DateTime<Utc>,

    /// When the token expires.
    pub expires_at: DateTime<Utc>,

    /// Whether the token has been consumed.
    pub consumed: bool,
}

#[allow(dead_code)]
impl BootstrapToken {
    /// Create a new bootstrap token entry from a token hash.
    pub fn new(token_hash: String, validity_hours: u64) -> Self {
        let now = Utc::now();
        Self {
            token_hash,
            created_at: now,
            expires_at: now + Duration::hours(validity_hours as i64),
            consumed: false,
        }
    }

    /// Check if the token is valid (not expired and not consumed).
    pub fn is_valid(&self) -> bool {
        !self.consumed && Utc::now() < self.expires_at
    }

    /// Mark the token as consumed.
    pub fn consume(&mut self) {
        self.consumed = true;
    }
}

/// Generate a new bootstrap token.
///
/// Returns a tuple of (plaintext_token, token_hash).
/// The plaintext token should be given to the worker admin.
/// The token_hash should be stored in the control plane.
#[allow(dead_code)]
pub fn generate_bootstrap_token() -> (String, String) {
    // Generate 256 bits (32 bytes) of random data
    let mut token_bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut token_bytes);

    // Encode as URL-safe base64
    let plaintext = URL_SAFE_NO_PAD.encode(token_bytes);

    // Hash for storage
    let token_hash = hash_token(&plaintext);

    (plaintext, token_hash)
}

/// Hash a token using SHA-256.
pub fn hash_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    let result = hasher.finalize();
    hex::encode(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_bootstrap_token() {
        let (plaintext, hash) = generate_bootstrap_token();

        // Token should be 43 chars (32 bytes base64 encoded)
        assert_eq!(plaintext.len(), 43);

        // Hash should be 64 chars (SHA-256 hex encoded)
        assert_eq!(hash.len(), 64);

        // Hashing the same token should produce the same hash
        assert_eq!(hash_token(&plaintext), hash);
    }

    #[test]
    fn test_token_validity() {
        let hash = "test_hash".to_string();
        let token = BootstrapToken::new(hash, 1);

        assert!(token.is_valid());
        assert!(!token.consumed);
    }

    #[test]
    fn test_token_consume() {
        let hash = "test_hash".to_string();
        let mut token = BootstrapToken::new(hash, 1);

        assert!(token.is_valid());
        token.consume();
        assert!(!token.is_valid());
        assert!(token.consumed);
    }
}
