//! AES-256-GCM encryption for API keys and OAuth tokens.
//!
//! Algorithm is identical to `skillstar-ai::translation_config::encrypt_api_key`
//! (machine_uid → SHA-256 → 32-byte key, random 12-byte nonce per encryption,
//! ciphertext stored as `base64(nonce || ciphertext)`). Kept in-crate to avoid
//! a heavy dependency on `skillstar-ai`.

use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, KeyInit},
};
use base64::{Engine, engine::general_purpose::STANDARD as BASE64};

fn get_encryption_key() -> [u8; 32] {
    let uid = machine_uid::get().unwrap_or_default();
    use sha2::{Digest, Sha256};
    let mut hash = Sha256::new();
    hash.update(uid.as_bytes());
    let result = hash.finalize();
    let mut key = [0u8; 32];
    key.copy_from_slice(&result);
    key
}

/// Encrypt a plaintext secret with AES-256-GCM. Returns base64 of `nonce || ciphertext`.
/// Returns an empty string when given empty input.
pub fn encrypt(plaintext: &str) -> String {
    if plaintext.is_empty() {
        return String::new();
    }
    let key = get_encryption_key();
    let cipher = Aes256Gcm::new_from_slice(&key).expect("AES-256-GCM accepts 32-byte key");
    let mut nonce_bytes = [0u8; 12];
    for byte in &mut nonce_bytes {
        *byte = rand::random::<u8>();
    }
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher
        .encrypt(nonce, plaintext.as_bytes())
        .expect("encryption should not fail with valid key/nonce");
    let mut combined = nonce_bytes.to_vec();
    combined.extend(ciphertext);
    BASE64.encode(&combined)
}

/// Decrypt a value previously produced by [`encrypt`]. Returns empty string
/// on any failure (corrupt input, tamper, wrong machine).
pub fn decrypt(encoded: &str) -> String {
    if encoded.is_empty() {
        return String::new();
    }
    let combined = match BASE64.decode(encoded) {
        Ok(c) => c,
        Err(_) => return String::new(),
    };
    if combined.len() < 12 {
        return String::new();
    }
    let key = get_encryption_key();
    let cipher = Aes256Gcm::new_from_slice(&key).expect("AES-256-GCM accepts 32-byte key");
    let (nonce_bytes, ciphertext) = combined.split_at(12);
    let nonce = Nonce::from_slice(nonce_bytes);
    match cipher.decrypt(nonce, ciphertext) {
        Ok(plaintext) => String::from_utf8(plaintext).unwrap_or_default(),
        Err(_) => String::new(),
    }
}

/// Convenience: encrypt only if `Some(value)`. Pass-through for `None`.
pub fn encrypt_opt(plaintext: Option<&str>) -> Option<String> {
    plaintext.map(encrypt)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let plain = "sk-test-secret-abc-123";
        let cipher = encrypt(plain);
        assert!(!cipher.is_empty());
        assert_ne!(cipher, plain);
        let decrypted = decrypt(&cipher);
        assert_eq!(decrypted, plain);
    }

    #[test]
    fn empty_passthrough() {
        assert_eq!(encrypt(""), "");
        assert_eq!(decrypt(""), "");
    }

    #[test]
    fn corrupt_returns_empty() {
        assert_eq!(decrypt("not-valid-base64!!!"), "");
        assert_eq!(decrypt("AAAA"), "");
    }
}
