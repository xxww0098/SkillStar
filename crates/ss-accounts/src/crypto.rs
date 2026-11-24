//! AES-256-GCM for the secrets an account record holds.
//!
//! Key = SHA-256(machine uid); a random 12-byte nonce per encryption; stored as
//! base64(nonce || ciphertext). The same scheme the deleted account code used,
//! kept because it is the one this machine can actually decrypt.

use std::sync::{Mutex, OnceLock};

use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, KeyInit},
};
use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use sha2::{Digest, Sha256};

/// A sealed secret this machine cannot open.
///
/// Typed rather than a bare string: callers branch on it (a record whose tokens
/// will not open needs a new login far more often than the file needs a bug
/// report).
#[derive(Debug, thiserror::Error)]
pub enum CryptoError {
    /// The stored text is not base64 at all.
    #[error("the sealed secret is not valid base64: {0}")]
    Encoding(String),

    /// Shorter than the 12-byte nonce it must start with.
    #[error("the sealed secret is shorter than its own nonce")]
    Truncated,

    /// Decryption failed: this machine's key did not produce it.
    #[error("this machine's key cannot open the sealed secret")]
    KeyMismatch,

    /// The plaintext is not UTF-8 text.
    #[error("the plaintext is not UTF-8: {0}")]
    NotUtf8(String),
}

static CACHED_KEY: OnceLock<[u8; 32]> = OnceLock::new();
static KEY_INIT_LOCK: Mutex<()> = Mutex::new(());

fn derive_key(uid: &str) -> [u8; 32] {
    let digest = Sha256::digest(uid.as_bytes());
    let mut key = [0_u8; 32];
    key.copy_from_slice(&digest);
    key
}

/// The 32-byte key this machine encrypts with.
///
/// Caches successful `machine_uid` lookups per process so macOS `ioreg` is not
/// spawned on every encrypt/decrypt; transient lookup errors stay uncached.
fn encryption_key() -> [u8; 32] {
    if let Some(key) = CACHED_KEY.get() {
        return *key;
    }
    let _guard = KEY_INIT_LOCK
        .lock()
        .expect("encryption key init lock poisoned");
    if let Some(key) = CACHED_KEY.get() {
        return *key;
    }
    match machine_uid::get() {
        Ok(uid) => {
            let key = derive_key(&uid);
            let _ = CACHED_KEY.set(key);
            key
        }
        Err(error) => {
            tracing::warn!(
                target: "accounts",
                error = %error,
                "machine_uid lookup failed; using uncached fallback key"
            );
            derive_key("")
        }
    }
}

/// Encrypt a plaintext secret; an empty input stays empty.
pub fn encrypt(plaintext: &str) -> String {
    if plaintext.is_empty() {
        return String::new();
    }
    let cipher = Aes256Gcm::new_from_slice(&encryption_key()).expect("32-byte key");
    let mut nonce_bytes = [0u8; 12];
    for byte in &mut nonce_bytes {
        *byte = rand::random::<u8>();
    }
    let ciphertext = cipher
        .encrypt(Nonce::from_slice(&nonce_bytes), plaintext.as_bytes())
        .expect("AES-256-GCM with a fresh nonce");
    let mut combined = nonce_bytes.to_vec();
    combined.extend(ciphertext);
    BASE64.encode(combined)
}

/// Decrypt what encrypt() produced.
pub fn decrypt(ciphertext: &str) -> Result<String, CryptoError> {
    if ciphertext.is_empty() {
        return Ok(String::new());
    }
    let raw = BASE64
        .decode(ciphertext)
        .map_err(|error| CryptoError::Encoding(error.to_string()))?;
    if raw.len() < 13 {
        return Err(CryptoError::Truncated);
    }
    let (nonce_bytes, body) = raw.split_at(12);
    let cipher =
        Aes256Gcm::new_from_slice(&encryption_key()).map_err(|_| CryptoError::KeyMismatch)?;
    let plaintext = cipher
        .decrypt(Nonce::from_slice(nonce_bytes), body)
        .map_err(|_| CryptoError::KeyMismatch)?;
    String::from_utf8(plaintext).map_err(|error| CryptoError::NotUtf8(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::{CryptoError, decrypt, encrypt};

    #[test]
    fn round_trips_and_never_leaks_plaintext() {
        let sealed = encrypt("sk-test-token");
        assert!(!sealed.contains("sk-test-token"));
        assert_eq!(decrypt(&sealed).unwrap(), "sk-test-token");
    }

    #[test]
    fn empty_stays_empty() {
        assert_eq!(encrypt(""), "");
        assert_eq!(decrypt("").unwrap(), "");
    }

    #[test]
    fn nonces_differ() {
        assert_ne!(encrypt("same"), encrypt("same"));
    }

    #[test]
    fn unreadable_secrets_report_why() {
        assert!(matches!(
            decrypt("not base64 at all!"),
            Err(CryptoError::Encoding(_))
        ));
        assert!(matches!(decrypt("AAAA"), Err(CryptoError::Truncated)));
        // Valid base64, wrong length for a nonce plus a tag, so it never opens.
        let mut tampered = encrypt("sk-test-token");
        tampered.push_str("AAAA");
        assert!(matches!(
            decrypt(&tampered),
            Err(CryptoError::Encoding(_)) | Err(CryptoError::KeyMismatch)
        ));
    }
}
