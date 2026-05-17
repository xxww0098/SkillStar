//! PKCE (RFC 7636) helpers.
//!
//! Standard S256 challenge: SHA-256(verifier) → base64url (no padding).

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use sha2::{Digest, Sha256};

fn fill_random(buf: &mut [u8]) {
    for byte in buf {
        *byte = rand::random::<u8>();
    }
}

/// A (verifier, challenge) pair. Hold onto the verifier until the token
/// exchange step.
#[derive(Debug, Clone)]
pub struct PkcePair {
    pub verifier: String,
    pub challenge: String,
}

impl PkcePair {
    /// Generate a fresh PKCE pair with a 32-byte random verifier.
    pub fn generate() -> Self {
        let mut bytes = [0u8; 32];
        fill_random(&mut bytes);
        let verifier = URL_SAFE_NO_PAD.encode(bytes);
        let challenge = challenge_from(&verifier);
        Self { verifier, challenge }
    }
}

pub fn challenge_from(verifier: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(verifier.as_bytes());
    URL_SAFE_NO_PAD.encode(hasher.finalize())
}

/// Random URL-safe state token for CSRF protection.
pub fn random_state() -> String {
    let mut bytes = [0u8; 24];
    fill_random(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_is_unique() {
        let a = PkcePair::generate();
        let b = PkcePair::generate();
        assert_ne!(a.verifier, b.verifier);
        assert_ne!(a.challenge, b.challenge);
    }

    #[test]
    fn challenge_matches_verifier() {
        let p = PkcePair::generate();
        assert_eq!(challenge_from(&p.verifier), p.challenge);
    }

    #[test]
    fn state_is_url_safe() {
        let s = random_state();
        // base64url alphabet only.
        assert!(s.chars().all(|c| {
            c.is_ascii_alphanumeric() || c == '-' || c == '_'
        }));
    }
}
