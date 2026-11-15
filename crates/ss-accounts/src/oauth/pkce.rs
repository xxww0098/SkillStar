//! PKCE (RFC 7636) verifier and S256 challenge.

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use sha2::{Digest, Sha256};

/// Bytes of entropy behind one verifier. Base64url of 64 bytes is 86
/// characters, comfortably inside RFC 7636's 43..=128 window.
const VERIFIER_BYTES: usize = 64;

/// A freshly generated PKCE pair.
#[derive(Debug, Clone)]
pub struct Pkce {
    pub verifier: String,
    pub challenge: String,
}

/// Generate a verifier (86 chars of unreserved ASCII) and its S256 challenge.
pub fn generate() -> Pkce {
    let verifier = random_verifier();
    let challenge = challenge_s256(&verifier);
    Pkce {
        verifier,
        challenge,
    }
}

/// The S256 challenge of `verifier` (`base64url(sha256(verifier))`, no
/// padding).
pub fn challenge_s256(verifier: &str) -> String {
    let digest = Sha256::digest(verifier.as_bytes());
    URL_SAFE_NO_PAD.encode(digest)
}

fn random_verifier() -> String {
    let mut bytes = [0_u8; VERIFIER_BYTES];
    for byte in &mut bytes {
        *byte = rand::random::<u8>();
    }
    URL_SAFE_NO_PAD.encode(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verifier_is_long_enough_and_url_safe() {
        let pkce = generate();
        assert!(pkce.verifier.len() >= 43, "verifier too short: {}", pkce.verifier.len());
        assert!(pkce.verifier.len() <= 128, "verifier too long: {}", pkce.verifier.len());
        assert!(
            pkce.verifier
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '~')),
            "verifier has characters outside RFC 7636's unreserved set: {}",
            pkce.verifier
        );
        assert!(!pkce.verifier.contains('='), "verifier must not be padded");
    }

    #[test]
    fn challenge_is_sha256_of_verifier() {
        let pkce = generate();
        assert_eq!(pkce.challenge, challenge_s256(&pkce.verifier));
        assert_eq!(pkce.challenge.len(), 43, "sha256 base64url is 43 chars");
        assert!(!pkce.challenge.contains('='));
    }

    #[test]
    fn rfc7636_appendix_b_vector() {
        // RFC 7636 Appendix B.
        assert_eq!(
            challenge_s256("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn verifiers_differ_between_calls() {
        assert_ne!(generate().verifier, generate().verifier);
    }
}
